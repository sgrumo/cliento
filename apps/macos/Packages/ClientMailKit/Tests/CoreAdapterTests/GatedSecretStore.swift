import CoreBridge
import Foundation
import Synchronization

/// A secret store whose `delete` waits until the test opens the gate, so a core call stays in
/// flight on one of the core's threads while the test makes other calls.
final class GatedSecretStore: SecretStoreBridge {
    private struct State {
        var entered = false
        var open = false
    }

    /// How long `delete` waits for the gate before giving up, so a broken test cannot hang.
    private static let timeout = Duration.seconds(5)

    private let state = Mutex(State())

    /// Whether the core has called `delete` and is waiting at the gate.
    var hasEntered: Bool { state.withLock { $0.entered } }

    /// Lets the waiting `delete` return.
    func open() {
        state.withLock { $0.open = true }
    }

    func read(label: String) throws -> String? {
        nil
    }

    func write(label: String, secret: String) throws {}

    func delete(label: String) throws -> Bool {
        state.withLock { $0.entered = true }
        let deadline = ContinuousClock.now + Self.timeout
        // This runs on a core thread, not in Swift concurrency, so blocking it is what the test wants.
        while !state.withLock({ $0.open }), ContinuousClock.now < deadline {
            Thread.sleep(forTimeInterval: 0.005)
        }
        return false
    }
}
