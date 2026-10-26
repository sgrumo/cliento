import CoreAdapter
import CoreBridge
import Foundation
import Security
import Testing

struct KeychainSecretStoreTests {
    private let label = "account-id:oauth-refresh-token"
    private let expectedQuery = FakeKeychain.Query(
        itemClass: kSecClassGenericPassword as String,
        service: "test.service",
        account: "account-id:oauth-refresh-token",
        synchronizable: false
    )

    @Test func writeAddsAGenericPasswordBoundToThisDevice() throws {
        let keychain = FakeKeychain()
        let store = KeychainSecretStore(client: keychain, service: "test.service")

        try store.write(label: label, secret: "token")
        let added = try #require(keychain.lastAdded)

        #expect(added.query == expectedQuery)
        #expect(added.accessible == kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly as String)
        #expect(added.data == Data("token".utf8))
    }

    @Test func readUpdateAndDeleteTargetTheSameItem() throws {
        let keychain = FakeKeychain()
        let store = KeychainSecretStore(client: keychain, service: "test.service")

        try store.write(label: label, secret: "old")
        try store.write(label: label, secret: "new")
        _ = try store.read(label: label)
        _ = try store.delete(label: label)

        #expect(keychain.lastQuery(of: .update) == expectedQuery)
        #expect(keychain.lastQuery(of: .copy) == expectedQuery)
        #expect(keychain.lastQuery(of: .delete) == expectedQuery)
    }

    @Test func writingAgainUpdatesTheExistingItem() throws {
        let keychain = FakeKeychain()
        let store = KeychainSecretStore(client: keychain)

        try store.write(label: label, secret: "old")
        try store.write(label: label, secret: "new")
        let read = try store.read(label: label)

        #expect(read == "new")
        #expect(keychain.updates == 1)
    }

    @Test func readOfAMissingItemIsNil() throws {
        let store = KeychainSecretStore(client: FakeKeychain())

        let read = try store.read(label: label)

        #expect(read == nil)
    }

    @Test func deleteReportsWhetherAnItemExisted() throws {
        let store = KeychainSecretStore(client: FakeKeychain())
        try store.write(label: label, secret: "token")

        let first = try store.delete(label: label)
        let second = try store.delete(label: label)
        let afterDelete = try store.read(label: label)

        #expect(first)
        #expect(!second)
        #expect(afterDelete == nil)
    }

    @Test func lockedKeychainIsUnavailableForReadAndDelete() {
        let keychain = FakeKeychain()
        keychain.failEveryCall(with: errSecInteractionNotAllowed)
        let store = KeychainSecretStore(client: keychain)

        #expect(throws: SecretBridgeError.Unavailable) {
            try store.read(label: label)
        }
        #expect(throws: SecretBridgeError.Unavailable) {
            try store.delete(label: label)
        }
    }

    @Test func otherFailuresCarryTheirStatus() {
        let keychain = FakeKeychain()
        keychain.failEveryCall(with: errSecAuthFailed)
        let store = KeychainSecretStore(client: keychain)

        #expect(throws: SecretBridgeError.Failure(code: errSecAuthFailed)) {
            try store.write(label: label, secret: "token")
        }
        #expect(throws: SecretBridgeError.Failure(code: errSecAuthFailed)) {
            try store.delete(label: label)
        }
    }

    @Test func failedUpdateAfterADuplicateIsReported() throws {
        let keychain = FakeKeychain()
        let store = KeychainSecretStore(client: keychain)
        try store.write(label: label, secret: "old")
        keychain.failUpdates(with: errSecAuthFailed)

        #expect(throws: SecretBridgeError.Failure(code: errSecAuthFailed)) {
            try store.write(label: label, secret: "new")
        }
    }

    @Test func successWithoutDataIsADecodeFailure() {
        let keychain = FakeKeychain()
        keychain.succeedCopiesWithoutData()
        let store = KeychainSecretStore(client: keychain)

        #expect(throws: SecretBridgeError.Failure(code: errSecDecode)) {
            try store.read(label: label)
        }
    }

    @Test func dataThatIsNotUTF8IsADecodeFailure() {
        let keychain = FakeKeychain()
        keychain.storeRaw(Data([0xFF, 0xFE]), label: label)
        let store = KeychainSecretStore(client: keychain)

        #expect(throws: SecretBridgeError.Failure(code: errSecDecode)) {
            try store.read(label: label)
        }
    }
}
