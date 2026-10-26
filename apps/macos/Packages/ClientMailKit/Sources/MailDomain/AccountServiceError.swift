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
    /// Signing in with this provider is not set up in this build.
    case signInUnavailable
    /// The sign-in was cancelled.
    case signInCancelled
    /// The user did not grant access at the provider.
    case signInDenied
    /// Signing in failed at the provider or on the way back from it.
    case signInFailed
    /// The server could not be reached. Retrying later can succeed.
    case network
    /// The user signed in but did not allow access to mail.
    case mailAccessNotGranted
}
