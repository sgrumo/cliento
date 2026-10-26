import CoreBridge
import MailDomain

/// Runs calls into the Rust core and translates their errors to ``AccountServiceError``.
enum CoreCall {
    /// Runs `body` and maps any thrown error.
    ///
    /// - Parameter body: A call into the generated bindings.
    /// - Returns: The value `body` returns.
    /// - Throws: The mapped ``AccountServiceError``.
    static func run<Value>(_ body: () throws -> Value) throws(AccountServiceError) -> Value {
        do {
            return try body()
        } catch {
            throw AccountServiceError(thrown: error)
        }
    }

    /// Awaits `body` and maps any thrown error.
    ///
    /// - Parameter body: An async call into the generated bindings.
    /// - Returns: The value `body` returns.
    /// - Throws: The mapped ``AccountServiceError``.
    static func run<Value>(_ body: () async throws -> Value) async throws(AccountServiceError)
        -> Value
    {
        do {
            return try await body()
        } catch {
            throw AccountServiceError(thrown: error)
        }
    }
}

extension AccountServiceError {
    /// Maps any error a call into the bindings throws.
    init(thrown error: any Error) {
        if let error = error as? CoreError {
            self.init(error)
        } else {
            // Panics and lifting failures in the bindings: treat as a storage failure.
            self = .storage
        }
    }

    /// Maps a core error to the app-facing error.
    init(_ error: CoreError) {
        switch error {
        case .InvalidAddress: self = .invalidAddress
        case .DuplicateAccount: self = .duplicateAccount
        // The UI never types ids, so an invalid id means the account is gone.
        case .AccountNotFound, .InvalidAccountId: self = .accountNotFound
        case .Storage: self = .storage
        case .SecretStorageUnavailable: self = .secretStorageUnavailable
        case .SecretStorage: self = .secretStorage
        // Only the constructors return it, and the app shows the same "could not open" screen.
        case .Runtime: self = .storage
        }
    }
}
