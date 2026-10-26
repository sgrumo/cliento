import CoreBridge
import MailDomain
import Testing

@testable import CoreAdapter

struct CoreCallTests {
    @Test func runtimeFailureMapsToStorage() {
        let mapped = AccountServiceError(CoreError.Runtime)

        #expect(mapped == .storage)
    }

    @Test(arguments: [
        (CoreError.SignInUnavailable, AccountServiceError.signInUnavailable),
        (CoreError.SignInCancelled, AccountServiceError.signInCancelled),
        (CoreError.SignInDenied, AccountServiceError.signInDenied),
        (CoreError.SignInFailed, AccountServiceError.signInFailed),
        (CoreError.SignInRequired, AccountServiceError.signInFailed),
        (CoreError.Network, AccountServiceError.network),
        (CoreError.MailAccessNotGranted, AccountServiceError.mailAccessNotGranted),
    ])
    func signInErrorsMap(core: CoreError, expected: AccountServiceError) {
        #expect(AccountServiceError(core) == expected)
    }

    @Test func errorsFromOutsideTheCoreMapToStorage() {
        let mapped = AccountServiceError(thrown: UndeclaredError())

        #expect(mapped == .storage)
    }
}
