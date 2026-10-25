import CoreBridge
import Foundation
import MailDomain

/// Implements ``AccountService`` with the Rust core.
///
/// Every call blocks on disk I/O, so each method is `@concurrent`: it always runs on the global
/// concurrent executor, never on the caller's actor.
public final class RustAccountService: AccountService {
    private let client: CoreClient

    private init(client: CoreClient) {
        self.client = client
    }

    /// Opens or creates the database at `databaseURL`, creating its folder when needed.
    ///
    /// - Parameter databaseURL: File URL of the SQLite database.
    /// - Throws: ``AccountServiceError/storage`` when the folder or database cannot be created.
    public convenience init(databaseURL: URL) throws(AccountServiceError) {
        let folder = databaseURL.deletingLastPathComponent()
        do {
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        } catch {
            throw .storage
        }
        let client = try CoreCall.run {
            try CoreClient.open(databasePath: databaseURL.path(percentEncoded: false))
        }
        self.init(client: client)
    }

    /// Creates a service backed by a private in-memory database, for tests and previews.
    ///
    /// - Throws: ``AccountServiceError/storage`` when the database cannot be created.
    public static func inMemory() throws(AccountServiceError) -> RustAccountService {
        RustAccountService(client: try CoreCall.run { try CoreClient.openInMemory() })
    }

    @concurrent
    public func accounts() async throws(AccountServiceError) -> [Account] {
        try CoreCall.run { try client.accounts() }.map(Account.init(record:))
    }

    @concurrent
    public func addAccount(address: String, displayName: String?) async throws(AccountServiceError)
        -> Account
    {
        let record = try CoreCall.run {
            try client.addAccount(address: address, displayName: displayName)
        }
        return Account(record: record)
    }

    @concurrent
    public func removeAccount(id: Account.ID) async throws(AccountServiceError) {
        try CoreCall.run { try client.removeAccount(id: id) }
    }
}
