import CoreBridge
import Foundation
import MailDomain

/// Implements ``AccountService`` with the Rust core.
///
/// The initialisers block on disk I/O. The methods await the core, which runs the work on its own
/// threads, so they never block the caller.
public final class RustAccountService: AccountService {
    private let client: CoreClient

    private init(client: CoreClient) {
        self.client = client
    }

    /// Opens or creates the database at `databaseURL`, creating its folder when needed.
    ///
    /// - Parameters:
    ///   - databaseURL: File URL of the SQLite database.
    ///   - secrets: Where the core keeps account secrets; the app passes a ``KeychainSecretStore``.
    /// - Throws: ``AccountServiceError/storage`` when the folder or database cannot be created.
    public convenience init(databaseURL: URL, secrets: any SecretStoreBridge) throws(AccountServiceError) {
        let folder = databaseURL.deletingLastPathComponent()
        do {
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        } catch {
            throw .storage
        }
        let client = try CoreCall.run {
            try CoreClient.open(databasePath: databaseURL.path(percentEncoded: false), secrets: secrets)
        }
        self.init(client: client)
    }

    /// Creates a service backed by a private in-memory database, for tests and previews.
    ///
    /// - Parameter secrets: Where the core keeps account secrets; in memory unless a test passes
    ///   its own store.
    /// - Throws: ``AccountServiceError/storage`` when the database cannot be created.
    public static func inMemory(
        secrets: any SecretStoreBridge = InMemorySecretStore()
    ) throws(AccountServiceError) -> RustAccountService {
        RustAccountService(client: try CoreCall.run { try CoreClient.openInMemory(secrets: secrets) })
    }

    public func accounts() async throws(AccountServiceError) -> [Account] {
        try await CoreCall.run { try await client.accounts() }.map(Account.init(record:))
    }

    public func addAccount(address: String, displayName: String?) async throws(AccountServiceError)
        -> Account
    {
        let record = try await CoreCall.run {
            try await client.addAccount(address: address, displayName: displayName)
        }
        return Account(record: record)
    }

    public func removeAccount(id: Account.ID) async throws(AccountServiceError) {
        try await CoreCall.run { try await client.removeAccount(id: id) }
    }
}
