import CoreBridge
import MailDomain
import Testing

@testable import CoreAdapter

struct CoreCallTests {
    @Test func runtimeFailureMapsToStorage() {
        let mapped = AccountServiceError(CoreError.Runtime)

        #expect(mapped == .storage)
    }

    @Test func errorsFromOutsideTheCoreMapToStorage() {
        let mapped = AccountServiceError(thrown: UndeclaredError())

        #expect(mapped == .storage)
    }
}
