import CoreAdapter
import Foundation
import Testing

/// Runs against the real login Keychain, so it is off by default. Run with `make keychain-test`.
///
/// Each run uses its own service name and deletes what it wrote, so it leaves nothing behind.
@Suite(.enabled(if: ProcessInfo.processInfo.environment["CLIENTMAIL_KEYCHAIN_TESTS"] == "1"))
struct SystemKeychainTests {
    @Test func writeReadUpdateAndDelete() throws {
        let store = KeychainSecretStore(
            client: SystemKeychain(),
            service: "dev.samuelemedici.clientmail.tests.\(UUID().uuidString)"
        )
        let label = "test-account:oauth-refresh-token"
        defer { _ = try? store.delete(label: label) }

        try store.write(label: label, secret: "first")
        let first = try store.read(label: label)
        try store.write(label: label, secret: "second")
        let second = try store.read(label: label)
        let deleted = try store.delete(label: label)
        let afterDelete = try store.read(label: label)

        #expect(first == "first")
        #expect(second == "second")
        #expect(deleted)
        #expect(afterDelete == nil)
    }
}
