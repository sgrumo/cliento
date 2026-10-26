import CoreBridge
import Foundation
import Security

/// Stores the core's secrets as generic-password items in the Keychain.
///
/// A platform adapter: it translates the core's calls into Keychain queries and holds no rules.
/// Items are readable after the first unlock (sync runs while the screen is locked) and bound to
/// this Mac: never synced through iCloud, never restored onto another Mac (decision record 0002).
public final class KeychainSecretStore: SecretStoreBridge {
    /// The Keychain service every ClientMail item uses.
    public static let defaultService = "dev.samuelemedici.clientmail"

    private let client: any KeychainClient
    private let service: String

    /// Creates the store.
    ///
    /// - Parameters:
    ///   - client: The Keychain to call; tests pass a fake.
    ///   - service: The service attribute of every item. Tests pass their own to stay isolated.
    public init(client: any KeychainClient, service: String = KeychainSecretStore.defaultService) {
        self.client = client
        self.service = service
    }

    public func read(label: String) throws -> String? {
        var query = itemQuery(label: label)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        let (status, data) = client.copyData(matching: query)
        switch status {
        case errSecSuccess:
            guard let data, let secret = String(bytes: data, encoding: .utf8) else {
                throw SecretBridgeError.Failure(code: errSecDecode)
            }
            return secret
        case errSecItemNotFound:
            return nil
        default:
            throw Self.error(for: status)
        }
    }

    public func write(label: String, secret: String) throws {
        let value: [String: Any] = [
            kSecValueData as String: Data(secret.utf8),
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
        ]
        let query = itemQuery(label: label)
        let added = client.add(query.merging(value) { _, new in new })
        switch added {
        case errSecSuccess:
            return
        case errSecDuplicateItem:
            let updated = client.update(query, with: value)
            guard updated == errSecSuccess else { throw Self.error(for: updated) }
        default:
            throw Self.error(for: added)
        }
    }

    public func delete(label: String) throws -> Bool {
        let status = client.delete(itemQuery(label: label))
        switch status {
        case errSecSuccess:
            return true
        case errSecItemNotFound:
            return false
        default:
            throw Self.error(for: status)
        }
    }

    /// Returns the attributes that identify one item.
    private func itemQuery(label: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: label,
            kSecAttrSynchronizable as String: false,
        ]
    }

    /// Maps a Keychain status to the error the core understands.
    private static func error(for status: OSStatus) -> SecretBridgeError {
        status == errSecInteractionNotAllowed ? .Unavailable : .Failure(code: status)
    }
}
