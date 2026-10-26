import MailDomain

/// In-memory ``AccountService`` for view model tests.
///
/// An actor, so the tests can share it with the main-actor view model without data races.
actor FakeAccountService: AccountService {
    private(set) var stored: [Account]
    private var nextFailure: AccountServiceError?
    private var googleSession: FakeSignInSession?
    private var onBegin: (@Sendable () async -> Void)?

    init(accounts: [Account] = []) {
        stored = accounts
    }

    /// Makes Google sign-in available, handing out `session`.
    func offerGoogleSignIn(_ session: FakeSignInSession) {
        googleSession = session
    }

    /// Runs `action` inside the next `beginGoogleSignIn`, before it returns.
    func onBeginGoogleSignIn(_ action: @escaping @Sendable () async -> Void) {
        onBegin = action
    }

    /// Stores an account directly, as if another path had added it.
    func store(_ account: Account) {
        stored.append(account)
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

    func isGoogleSignInAvailable() -> Bool {
        googleSession != nil
    }

    func beginGoogleSignIn() async throws(AccountServiceError) -> any SignInSession {
        if let onBegin {
            self.onBegin = nil
            await onBegin()
        }
        try consumeFailure()
        guard let googleSession else { throw .signInUnavailable }
        return googleSession
    }

    private func consumeFailure() throws(AccountServiceError) {
        if let failure = nextFailure {
            nextFailure = nil
            throw failure
        }
    }
}
