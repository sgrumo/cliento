import CoreAdapter
import CoreBridge
import Foundation
import MailDomain
import Testing

/// Runs against the real Rust core with private databases.
struct RustAccountServiceTests {
    @Test func addListAndRemoveAccount() async throws {
        let service = try RustAccountService.inMemory()

        let added = try await service.addAccount(address: " Ada@Gmail.com ", displayName: "Ada")
        let listed = try await service.accounts()
        try await service.removeAccount(id: added.id)
        let afterRemoval = try await service.accounts()

        #expect(added.address == "Ada@gmail.com")
        #expect(added.provider == .gmail)
        #expect(listed == [added])
        #expect(afterRemoval.isEmpty)
    }

    @Test func duplicateAddressMapsToDuplicateAccount() async throws {
        let service = try RustAccountService.inMemory()
        _ = try await service.addAccount(address: "ada@example.com", displayName: nil)

        await #expect(throws: AccountServiceError.duplicateAccount) {
            try await service.addAccount(address: "ada@example.com", displayName: nil)
        }
    }

    @Test func invalidAddressMapsToInvalidAddress() async throws {
        let service = try RustAccountService.inMemory()

        await #expect(throws: AccountServiceError.invalidAddress) {
            try await service.addAccount(address: "nope", displayName: nil)
        }
    }

    @Test func removingUnknownIdMapsToAccountNotFound() async throws {
        let service = try RustAccountService.inMemory()

        await #expect(throws: AccountServiceError.accountNotFound) {
            try await service.removeAccount(id: "not-a-uuid")
        }
    }

    @Test func accountsPersistAcrossReopen() async throws {
        let folder = URL.temporaryDirectory.appending(path: "clientmail-tests-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let databaseURL = folder.appending(path: "mail.sqlite")

        let secrets = InMemorySecretStore()

        let added = try await RustAccountService(databaseURL: databaseURL, secrets: secrets)
            .addAccount(address: "ada@example.com", displayName: nil)
        let reopened = try await RustAccountService(databaseURL: databaseURL, secrets: secrets)
            .accounts()

        #expect(reopened == [added])
    }

    @Test func removingAnAccountDeletesItsSecret() async throws {
        let secrets = InMemorySecretStore()
        let service = try RustAccountService.inMemory(secrets: secrets)
        let added = try await service.addAccount(address: "ada@example.com", displayName: nil)
        let label = "\(added.id):oauth-refresh-token"
        try secrets.write(label: label, secret: "token")

        try await service.removeAccount(id: added.id)
        let afterRemoval = try secrets.read(label: label)

        #expect(afterRemoval == nil)
    }

    @Test(arguments: [
        (SecretBridgeError.Unavailable, AccountServiceError.secretStorageUnavailable),
        (SecretBridgeError.Failure(code: -25_293), AccountServiceError.secretStorage),
    ])
    func keychainFailuresKeepTheAccountAndReachSwift(
        thrown: SecretBridgeError,
        expected: AccountServiceError
    ) async throws {
        let service = try RustAccountService.inMemory(secrets: FailingSecretStore(throwing: thrown))
        let added = try await service.addAccount(address: "ada@example.com", displayName: nil)

        await #expect(throws: expected) {
            try await service.removeAccount(id: added.id)
        }
        let remaining = try await service.accounts()

        #expect(remaining == [added])
    }

    @Test func undeclaredSwiftErrorsAreSecretStorageFailures() async throws {
        let service = try RustAccountService.inMemory(
            secrets: FailingSecretStore(throwing: UndeclaredError())
        )
        let added = try await service.addAccount(address: "ada@example.com", displayName: nil)

        await #expect(throws: AccountServiceError.secretStorage) {
            try await service.removeAccount(id: added.id)
        }
        let remaining = try await service.accounts()

        #expect(remaining == [added])
    }
}
