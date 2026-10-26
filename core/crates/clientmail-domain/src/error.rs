use thiserror::Error;

/// Errors raised when input does not satisfy a domain rule.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[expect(
    clippy::enum_variant_names,
    reason = "every variant is a validation failure; dropping the prefix makes call sites unclear"
)]
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
    /// The text is not a valid mailbox identifier.
    #[error("invalid mailbox id")]
    InvalidMailboxId,
    /// The text is not a valid message identifier.
    #[error("invalid message id")]
    InvalidMessageId,
    /// The mailbox name breaks a naming rule. The message never includes the name itself.
    #[error("invalid mailbox path: {reason}")]
    InvalidMailboxPath {
        /// Which rule the name broke.
        reason: &'static str,
    },
    /// The text is not a valid IMAP keyword.
    #[error("invalid keyword")]
    InvalidKeyword,
    /// The text is not 64 hex characters naming a blob.
    #[error("invalid blob id")]
    InvalidBlobId,
}
