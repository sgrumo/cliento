import CoreBridge

/// A secret store whose every call throws `error`, to drive the core's failure paths.
final class FailingSecretStore: SecretStoreBridge {
    private let error: any Error & Sendable

    init(throwing error: any Error & Sendable) {
        self.error = error
    }

    func read(label: String) throws -> String? {
        throw error
    }

    func write(label: String, secret: String) throws {
        throw error
    }

    func delete(label: String) throws -> Bool {
        throw error
    }
}
