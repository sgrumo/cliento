import CoreBridge
import Synchronization

/// Keeps secrets in memory, for SwiftUI previews and tests. Nothing survives the process.
public final class InMemorySecretStore: SecretStoreBridge {
    /// `Mutex` makes the dictionary safe to use from the core's threads.
    private let items = Mutex<[String: String]>([:])

    /// Creates an empty store.
    public init() {}

    public func read(label: String) throws -> String? {
        items.withLock { $0[label] }
    }

    public func write(label: String, secret: String) throws {
        items.withLock { $0[label] = secret }
    }

    public func delete(label: String) throws -> Bool {
        items.withLock { $0.removeValue(forKey: label) != nil }
    }
}
