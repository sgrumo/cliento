import Foundation

/// The four Keychain calls ``KeychainSecretStore`` needs, so tests can replace the system Keychain.
///
/// Queries use the `kSec...` keys of the Security framework. Implementations must be thread-safe:
/// the Rust core calls the store from its own threads.
public protocol KeychainClient: Sendable {
    /// Adds an item. Returns `errSecDuplicateItem` when it already exists.
    func add(_ attributes: [String: Any]) -> OSStatus

    /// Updates the items matching `query` with `attributes`.
    func update(_ query: [String: Any], with attributes: [String: Any]) -> OSStatus

    /// Returns the data of the item matching `query`, with the call's status.
    func copyData(matching query: [String: Any]) -> (status: OSStatus, data: Data?)

    /// Deletes the items matching `query`. Returns `errSecItemNotFound` when there are none.
    func delete(_ query: [String: Any]) -> OSStatus
}
