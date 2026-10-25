use thiserror::Error;

/// Errors raised when input does not satisfy a domain rule.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DomainError {
    /// The text is not a valid email address.
    #[error("invalid email address: {reason}")]
    InvalidEmailAddress {
        /// Why the address was rejected.
        reason: String,
    },
    /// The text is not a valid account identifier.
    #[error("invalid account id")]
    InvalidAccountId,
}
