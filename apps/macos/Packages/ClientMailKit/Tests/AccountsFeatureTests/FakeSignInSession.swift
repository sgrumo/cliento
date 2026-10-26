import Foundation
import MailDomain
import Synchronization

/// A ``SignInSession`` that completes with a fixed result and records cancels and callbacks.
final class FakeSignInSession: SignInSession {
    private struct State {
        var cancels = 0
        var callbackURL: URL?
    }

    let authorizationURL = URL(filePath: "/fake/authorize")
    let callbackScheme = "test"
    private let result: Result<Account, AccountServiceError>
    private let state = Mutex(State())

    /// - Parameter result: What ``complete(callbackURL:)`` returns or throws.
    init(result: Result<Account, AccountServiceError>) {
        self.result = result
    }

    /// How often ``cancel()`` was called.
    var cancels: Int { state.withLock { $0.cancels } }

    /// The URL ``complete(callbackURL:)`` received, if it was called.
    var callbackURL: URL? { state.withLock { $0.callbackURL } }

    func complete(callbackURL: URL) throws(AccountServiceError) -> Account {
        state.withLock { $0.callbackURL = callbackURL }
        return try result.get()
    }

    func cancel() {
        state.withLock { $0.cancels += 1 }
    }
}
