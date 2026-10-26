use std::error::Error;

use clientmail_domain::ports::OAuthClient;
use clientmail_domain::{DomainError, OAuthErrorKind};
use thiserror::Error;

/// Errors returned by the engine's use cases.
///
/// Messages never include addresses or other personal data, because they can end up in logs.
#[derive(Debug, Error)]
pub enum EngineError {
    /// The input broke a domain rule.
    #[error(transparent)]
    InvalidInput(#[from] DomainError),
    /// An account with the same address already exists.
    #[error("an account with this address already exists")]
    DuplicateAccount,
    /// No account has the given id.
    #[error("account not found")]
    AccountNotFound,
    /// Storage failed. The source holds the storage-specific error.
    #[error("storage failed")]
    Storage(#[source] Box<dyn Error + Send + Sync>),
    /// Secret storage (the Keychain on macOS) failed. The source holds the store's error.
    #[error("secret storage failed")]
    Secrets(#[source] Box<dyn Error + Send + Sync>),
    /// The user refused consent at the provider.
    #[error("sign-in was denied")]
    SignInDenied,
    /// Signing in failed: a mismatched callback, an incomplete grant, or a provider error. The
    /// source says which.
    #[error("sign-in failed")]
    SignInFailed(#[source] Box<dyn Error + Send + Sync>),
    /// The account has no usable refresh token; the user must sign in again.
    #[error("sign-in required")]
    SignInRequired,
    /// The provider could not be reached. The source holds the network error.
    #[error("network unavailable")]
    Network(#[source] Box<dyn Error + Send + Sync>),
    /// The user signed in but did not grant access to mail, so the account could not sync.
    #[error("mail access was not granted")]
    MailAccessNotGranted,
}

/// Why a sign-in failed, when the provider itself reported no error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SignInProblem {
    /// The grant has no refresh token, so the account could not sync later.
    #[error("the provider issued no refresh token")]
    NoRefreshToken,
    /// The grant does not say who signed in.
    #[error("the provider did not identify the account")]
    NoIdentity,
}

impl EngineError {
    /// Wraps any storage error. Used with `map_err` at each repository call.
    pub(crate) fn storage(error: impl Error + Send + Sync + 'static) -> Self {
        Self::Storage(Box::new(error))
    }

    /// Wraps any secret-store error. Used with `map_err` at each secret-store call.
    pub(crate) fn secrets(error: impl Error + Send + Sync + 'static) -> Self {
        Self::Secrets(Box::new(error))
    }

    /// Classifies an OAuth provider's error for a sign-in or a refresh.
    ///
    /// `Rejected` means different things: during sign-in the code was refused (the sign-in
    /// failed); during a refresh the stored token was refused (sign in again).
    pub(crate) fn oauth<O: OAuthClient>(error: O::Error, refreshing: bool) -> Self {
        match O::error_kind(&error) {
            OAuthErrorKind::Denied => Self::SignInDenied,
            OAuthErrorKind::Rejected if refreshing => Self::SignInRequired,
            OAuthErrorKind::Network => Self::Network(Box::new(error)),
            OAuthErrorKind::Rejected | OAuthErrorKind::Protocol => {
                Self::SignInFailed(Box::new(error))
            }
        }
    }
}
