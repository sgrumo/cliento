import MailDomain

/// In-memory ``AccountService`` for view model tests.
///
/// An actor, so the tests can share it with the main-actor view model without data races.
actor FakeAccountService: AccountService {
    private(set) var stored: [Account]
    private var nextFailure: AccountServiceError?

    init(accounts: [Account] = []) {
        stored = accounts
    }

    /// Makes the next call throw `error`.
    func failNextCall(with error: AccountServiceError) {
        nextFailure = error
    }

    func accounts() throws(AccountServiceError) -> [Account] {
        try consumeFailure()
        return stored
    }

    func addAccount(address: String, displayName: String?) throws(AccountServiceError) -> Account {
        try consumeFailure()
        let account = Account(
            id: "\(stored.count + 1)",
            address: address,
            displayName: displayName,
            provider: .genericImap
        )
        stored.append(account)
        return account
    }

    func removeAccount(id: Account.ID) throws(AccountServiceError) {
        try consumeFailure()
        stored.removeAll { $0.id == id }
    }

    private func consumeFailure() throws(AccountServiceError) {
        if let failure = nextFailure {
            nextFailure = nil
            throw failure
        }
    }
}
