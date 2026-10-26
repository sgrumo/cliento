import AccountsFeature
import CoreAdapter
import Foundation
import MailDomain

/// The result of wiring the app's dependencies. This is the composition root.
enum Startup {
    /// Dependencies are ready.
    case ready(AccountsViewModel)
    /// The local database could not be opened.
    case failed(AccountServiceError)

    /// Opens the database in Application Support and builds the feature view models.
    static func launch() -> Startup {
        let databaseURL = URL.applicationSupportDirectory
            .appending(path: "ClientMail", directoryHint: .isDirectory)
            .appending(path: "mail.sqlite")
        do {
            let service = try RustAccountService(
                databaseURL: databaseURL,
                secrets: KeychainSecretStore(client: SystemKeychain())
            )
            return .ready(AccountsViewModel(service: service))
        } catch {
            return .failed(error)
        }
    }
}
