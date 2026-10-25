use clientmail_domain::DomainError;
use clientmail_engine::EngineError;
use clientmail_store::StoreError;

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
            | DomainError::InvalidKeyword => Self::Storage,
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
    fn mail_domain_errors_map_to_storage() {
        for error in [
            DomainError::InvalidMailboxId,
            DomainError::InvalidMessageId,
            DomainError::InvalidMailboxPath { reason: "test" },
            DomainError::InvalidKeyword,
        ] {
            assert_eq!(
                CoreError::from(error.clone()),
                CoreError::Storage,
                "{error:?}"
            );
        }
    }
}
