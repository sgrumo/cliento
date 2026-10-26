import CoreAdapter
import Foundation
import LocalAuthentication
import Security
import Synchronization

/// In-memory ``KeychainClient`` that behaves like the Keychain for one service, records the query
/// of every call, and can be told to fail.
final class FakeKeychain: KeychainClient {
    /// A Keychain call.
    enum Operation: Hashable, Sendable {
        case add, update, copy, delete
    }

    /// The identifying attributes of a query. `Sendable`, unlike `[String: Any]`.
    struct Query: Sendable, Equatable {
        var itemClass: String?
        var service: String?
        var account: String?
        var synchronizable: Bool?
    }

    /// The value attributes of an added item.
    struct AddedItem: Sendable {
        var query: Query
        var accessible: String?
        var data: Data?
    }

    private struct State {
        var items: [String: Data] = [:]
        var queries: [Operation: Query] = [:]
        var interactionNotAllowed: [Operation: Bool] = [:]
        var lastAdded: AddedItem?
        var updates = 0
        var failure: OSStatus?
        var updateFailure: OSStatus?
        var copySucceedsWithoutData = false
    }

    private let state = Mutex(State())

    /// Makes every later call return `status`.
    func failEveryCall(with status: OSStatus) {
        state.withLock { $0.failure = status }
    }

    /// Makes later `update` calls return `status`; other calls keep working.
    func failUpdates(with status: OSStatus) {
        state.withLock { $0.updateFailure = status }
    }

    /// Makes later `copyData` calls report success without returning any data.
    func succeedCopiesWithoutData() {
        state.withLock { $0.copySucceedsWithoutData = true }
    }

    /// Stores raw bytes under `label`, bypassing the store under test.
    func storeRaw(_ data: Data, label: String) {
        state.withLock { $0.items[label] = data }
    }

    /// The query of the last call of `operation`.
    func lastQuery(of operation: Operation) -> Query? {
        state.withLock { $0.queries[operation] }
    }

    /// Whether the last call of `operation` carried an authentication context that forbids
    /// prompts; `nil` when it carried no context.
    func interactionNotAllowed(in operation: Operation) -> Bool? {
        state.withLock { $0.interactionNotAllowed[operation] }
    }

    /// The attributes of the last successful `add`.
    var lastAdded: AddedItem? { state.withLock { $0.lastAdded } }

    /// How many successful `update` calls happened.
    var updates: Int { state.withLock { $0.updates } }

    func add(_ attributes: [String: Any]) -> OSStatus {
        record(.add, attributes)
        let query = Self.query(of: attributes)
        let item = AddedItem(
            query: query,
            accessible: attributes[kSecAttrAccessible as String] as? String,
            data: attributes[kSecValueData as String] as? Data
        )
        return state.withLock { state in
            state.queries[.add] = query
            if let failure = state.failure { return failure }
            let label = query.account ?? ""
            guard state.items[label] == nil else { return errSecDuplicateItem }
            state.items[label] = item.data
            state.lastAdded = item
            return errSecSuccess
        }
    }

    func update(_ query: [String: Any], with attributes: [String: Any]) -> OSStatus {
        record(.update, query)
        let recorded = Self.query(of: query)
        let data = attributes[kSecValueData as String] as? Data
        return state.withLock { state in
            state.queries[.update] = recorded
            if let failure = state.failure ?? state.updateFailure { return failure }
            let label = recorded.account ?? ""
            guard state.items[label] != nil else { return errSecItemNotFound }
            state.items[label] = data
            state.updates += 1
            return errSecSuccess
        }
    }

    func copyData(matching query: [String: Any]) -> (status: OSStatus, data: Data?) {
        record(.copy, query)
        let recorded = Self.query(of: query)
        return state.withLock { state in
            state.queries[.copy] = recorded
            if let failure = state.failure { return (failure, nil) }
            if state.copySucceedsWithoutData { return (errSecSuccess, nil) }
            guard let data = state.items[recorded.account ?? ""] else {
                return (errSecItemNotFound, nil)
            }
            return (errSecSuccess, data)
        }
    }

    func delete(_ query: [String: Any]) -> OSStatus {
        record(.delete, query)
        let recorded = Self.query(of: query)
        return state.withLock { state in
            state.queries[.delete] = recorded
            if let failure = state.failure { return failure }
            return state.items.removeValue(forKey: recorded.account ?? "") == nil
                ? errSecItemNotFound : errSecSuccess
        }
    }

    /// Records the call's authentication context; `LAContext` is not `Sendable`, so only its flag
    /// is kept.
    private func record(_ operation: Operation, _ attributes: [String: Any]) {
        let context = attributes[kSecUseAuthenticationContext as String] as? LAContext
        state.withLock { $0.interactionNotAllowed[operation] = context?.interactionNotAllowed }
    }

    private static func query(of attributes: [String: Any]) -> Query {
        Query(
            itemClass: attributes[kSecClass as String] as? String,
            service: attributes[kSecAttrService as String] as? String,
            account: attributes[kSecAttrAccount as String] as? String,
            synchronizable: attributes[kSecAttrSynchronizable as String] as? Bool
        )
    }
}
