use std::error::Error;

use clientmail_domain::DomainError;
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
}

impl EngineError {
    /// Wraps any storage error. Used with `map_err` at each repository call.
    pub(crate) fn storage(error: impl Error + Send + Sync + 'static) -> Self {
        Self::Storage(Box::new(error))
    }
}
