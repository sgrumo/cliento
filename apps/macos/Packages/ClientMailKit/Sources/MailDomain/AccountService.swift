/// Manages the accounts configured in the app.
///
/// Features depend on this protocol only. `CoreAdapter` implements it with the Rust core, and
/// tests implement it with fakes.
public protocol AccountService: Sendable {
    /// Returns every account, oldest first.
    func accounts() async throws(AccountServiceError) -> [Account]

    /// Validates and stores a new account.
    ///
    /// - Parameters:
    ///   - address: The email address as typed.
    ///   - displayName: The sender name, or `nil`.
    /// - Returns: The stored account.
    func addAccount(address: String, displayName: String?) async throws(AccountServiceError)
        -> Account

    /// Removes an account.
    ///
    /// - Parameter id: The id of the account to remove.
    func removeAccount(id: Account.ID) async throws(AccountServiceError)
}
