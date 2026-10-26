/// Failures of account operations, independent of how the core reports them.
public enum AccountServiceError: Error, Hashable, Sendable {
    /// The email address is not valid.
    case invalidAddress
    /// An account with this address already exists.
    case duplicateAccount
    /// The account no longer exists.
    case accountNotFound
    /// The local database failed.
    case storage
    /// The Keychain cannot be used right now, for example because it is locked.
    case secretStorageUnavailable
    /// The Keychain failed in a way retrying will not fix.
    case secretStorage
}
