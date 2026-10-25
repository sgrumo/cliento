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
}
