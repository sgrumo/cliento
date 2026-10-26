import Foundation
import Security

/// ``KeychainClient`` backed by the macOS Keychain through the `SecItem` functions.
///
/// `SecItem` calls are thread-safe and may block, so they must never run on the main actor.
public struct SystemKeychain: KeychainClient {
    /// Creates the client.
    public init() {}

    public func add(_ attributes: [String: Any]) -> OSStatus {
        SecItemAdd(attributes as CFDictionary, nil)
    }

    public func update(_ query: [String: Any], with attributes: [String: Any]) -> OSStatus {
        SecItemUpdate(query as CFDictionary, attributes as CFDictionary)
    }

    public func copyData(matching query: [String: Any]) -> (status: OSStatus, data: Data?) {
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        return (status, result as? Data)
    }

    public func delete(_ query: [String: Any]) -> OSStatus {
        SecItemDelete(query as CFDictionary)
    }
}
