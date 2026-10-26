use thiserror::Error;

/// Errors raised by the `SQLite` store.
#[derive(Debug, Error)]
pub enum StoreError {
    /// `SQLite` reported an error.
    #[error("database error")]
    Sqlite(#[from] rusqlite::Error),
    /// A stored row holds a value the domain rejects.
    #[error("corrupt value in column `{column}`")]
    CorruptRow {
        /// The column that holds the bad value.
        column: &'static str,
    },
    /// The database lock was poisoned by a panic in another thread.
    #[error("database lock poisoned")]
    LockPoisoned,
    /// No mailbox has the id a write referred to.
    #[error("mailbox not found")]
    MissingMailbox,
    /// No message has the id a write referred to.
    #[error("message not found")]
    MissingMessage,
    /// Labels were set on a plain IMAP message, whose folder is part of its server reference.
    #[error("labels apply to Gmail messages only")]
    LabelsOnImapMessage,
    /// A migration left a reference to a row that does not exist.
    #[error("migration broke a foreign key")]
    ForeignKeyViolation,
    /// Reading or writing a blob file failed.
    #[error("blob file error")]
    Io(#[from] std::io::Error),
    /// A blob file no longer matches the hash it is named after.
    #[error("blob content does not match its id")]
    CorruptBlob,
}
