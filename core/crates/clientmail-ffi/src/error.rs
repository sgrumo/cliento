use clientmail_domain::DomainError;
use clientmail_engine::EngineError;
use clientmail_store::StoreError;

use crate::SecretBridgeError;

/// Errors the Swift app can receive. Each case maps to a message the UI can show.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum CoreError {
    /// The email address is not valid.
    #[error("invalid email address")]
    InvalidAddress,
    /// The account id sent from Swift is not valid.
    #[error("invalid account id")]
    InvalidAccountId,
    /// An account with this address already exists.
    #[error("an account with this address already exists")]
    DuplicateAccount,
    /// No account has this id.
    #[error("account not found")]
    AccountNotFound,
    /// The local database failed.
    #[error("storage failed")]
    Storage,
    /// The platform's secret storage (the Keychain on macOS) cannot be used right now, for example
    /// because it is locked. Retrying later can succeed.
    #[error("secret storage unavailable")]
    SecretStorageUnavailable,
    /// The platform's secret storage (the Keychain on macOS) failed. Retrying will not help.
    #[error("secret storage failed")]
    SecretStorage,
    /// The core could not set up its runtime. A thread the operating system refuses to start
    /// panics instead, inside Tokio.
    #[error("the core could not start")]
    Runtime,
    /// Google sign-in is not configured: no valid client id.
    #[error("sign-in is not available")]
    SignInUnavailable,
    /// The sign-in was cancelled.
    #[error("sign-in was cancelled")]
    SignInCancelled,
    /// The user refused consent at the provider.
    #[error("sign-in was denied")]
    SignInDenied,
    /// Signing in failed at the provider or on the way back from it.
    #[error("sign-in failed")]
    SignInFailed,
    /// The account's sign-in expired or was revoked; the user must sign in again.
    #[error("sign-in required")]
    SignInRequired,
    /// The provider could not be reached. Retrying later can succeed.
    #[error("network unavailable")]
    Network,
    /// The user signed in but did not allow access to mail.
    #[error("mail access was not granted")]
    MailAccessNotGranted,
}

impl From<DomainError> for CoreError {
    fn from(error: DomainError) -> Self {
        match error {
            DomainError::InvalidEmailAddress { .. } => Self::InvalidAddress,
            DomainError::InvalidAccountId => Self::InvalidAccountId,
            // No Swift input reaches these. They come from stored data or, from step 1.6, from
            // server data; 1.6 adds a dedicated case so bad server data is not shown as storage.
            DomainError::InvalidMailboxId
            | DomainError::InvalidMessageId
            | DomainError::InvalidMailboxPath { .. }
            | DomainError::InvalidKeyword
            | DomainError::InvalidBlobId => Self::Storage,
            // Only generated verifiers reach this, so it means sign-in could not start.
            DomainError::InvalidPkceVerifier => Self::SignInFailed,
        }
    }
}

impl From<EngineError> for CoreError {
    fn from(error: EngineError) -> Self {
        match error {
            EngineError::InvalidInput(domain) => domain.into(),
            EngineError::DuplicateAccount => Self::DuplicateAccount,
            EngineError::AccountNotFound => Self::AccountNotFound,
            EngineError::Storage(_) => Self::Storage,
            EngineError::SignInDenied => Self::SignInDenied,
            EngineError::SignInFailed(_) => Self::SignInFailed,
            EngineError::SignInRequired => Self::SignInRequired,
            EngineError::Network(_) => Self::Network,
            EngineError::MailAccessNotGranted => Self::MailAccessNotGranted,
            // The engine boxes the secret store's error; for this crate that store is the bridge, so
            // `downcast_ref` recovers its `SecretBridgeError` to tell locked from broken.
            EngineError::Secrets(source) => match source.downcast_ref::<SecretBridgeError>() {
                Some(SecretBridgeError::Unavailable) => Self::SecretStorageUnavailable,
                Some(SecretBridgeError::Failure { .. } | SecretBridgeError::Unexpected) | None => {
                    Self::SecretStorage
                }
            },
        }
    }
}

impl From<StoreError> for CoreError {
    fn from(_: StoreError) -> Self {
        Self::Storage
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_errors_map_to_ffi_cases() {
        assert_eq!(
            CoreError::from(EngineError::DuplicateAccount),
            CoreError::DuplicateAccount,
            "duplicate"
        );
        assert_eq!(
            CoreError::from(EngineError::AccountNotFound),
            CoreError::AccountNotFound,
            "not found"
        );
        assert_eq!(
            CoreError::from(EngineError::InvalidInput(DomainError::InvalidAccountId)),
            CoreError::InvalidAccountId,
            "invalid id"
        );
    }

    #[test]
    fn secret_errors_tell_locked_from_broken() {
        let error =
            |bridge: SecretBridgeError| CoreError::from(EngineError::Secrets(Box::new(bridge)));

        assert_eq!(
            error(SecretBridgeError::Unavailable),
            CoreError::SecretStorageUnavailable,
            "locked"
        );
        assert_eq!(
            error(SecretBridgeError::Failure { code: -25_293 }),
            CoreError::SecretStorage,
            "failed"
        );
        assert_eq!(
            error(SecretBridgeError::Unexpected),
            CoreError::SecretStorage,
            "unexpected"
        );
    }

    #[test]
    fn sign_in_errors_map_to_their_cases() {
        let source = || Box::new(std::fmt::Error) as Box<dyn std::error::Error + Send + Sync>;
        let cases = [
            (EngineError::SignInDenied, CoreError::SignInDenied),
            (EngineError::SignInFailed(source()), CoreError::SignInFailed),
            (EngineError::SignInRequired, CoreError::SignInRequired),
            (EngineError::Network(source()), CoreError::Network),
            (
                EngineError::MailAccessNotGranted,
                CoreError::MailAccessNotGranted,
            ),
            (
                EngineError::InvalidInput(DomainError::InvalidPkceVerifier),
                CoreError::SignInFailed,
            ),
        ];

        for (engine, expected) in cases {
            assert_eq!(CoreError::from(engine), expected, "{expected:?}");
        }
    }

    #[test]
    fn mail_domain_errors_map_to_storage() {
        for error in [
            DomainError::InvalidMailboxId,
            DomainError::InvalidMessageId,
            DomainError::InvalidMailboxPath { reason: "test" },
            DomainError::InvalidKeyword,
            DomainError::InvalidBlobId,
        ] {
            assert_eq!(
                CoreError::from(error.clone()),
                CoreError::Storage,
                "{error:?}"
            );
        }
    }
}
