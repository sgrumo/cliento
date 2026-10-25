use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use rusqlite::Connection;

use crate::StoreError;
use crate::migrations::migrate;

/// A `SQLite` database holding the local mail state.
///
/// `rusqlite::Connection` can move between threads (`Send`) but not be shared (`!Sync`). The
/// `Mutex` makes the store shareable; each call holds the lock only for its own statements.
#[derive(Debug)]
pub struct SqliteStore {
    connection: Mutex<Connection>,
}

impl SqliteStore {
    /// Opens or creates the database file and applies pending migrations.
    ///
    /// # Arguments
    ///
    /// * `path` - The database file. Its parent folder must exist.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Sqlite`] when the file cannot be opened or migrated.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        Self::from_connection(Connection::open(path)?)
    }

    /// Opens a private in-memory database. Used by tests and previews.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Sqlite`] when migrations fail.
    pub fn open_in_memory() -> Result<Self, StoreError> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(mut connection: Connection) -> Result<Self, StoreError> {
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "foreign_keys", true)?;
        migrate(&mut connection)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    /// Runs `work` while holding the database lock, and releases the lock as soon as it returns.
    ///
    /// # Arguments
    ///
    /// * `work` - The statements to run. `FnOnce` means the closure is called exactly once.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::LockPoisoned`] when another thread panicked while holding the lock,
    /// or the error `work` returns.
    pub(crate) fn with_connection<T>(
        &self,
        work: impl FnOnce(&Connection) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        let guard: MutexGuard<'_, Connection> = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        work(&guard)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_creates_file_and_reopens_it() {
        let folder = std::env::temp_dir().join(format!("clientmail-store-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("mail.sqlite");

        SqliteStore::open(&path).unwrap();
        let reopened = SqliteStore::open(&path);

        assert!(reopened.is_ok(), "reopening an existing database must work");
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
