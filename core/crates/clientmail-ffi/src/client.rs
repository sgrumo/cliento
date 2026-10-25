use std::path::Path;
use std::sync::Arc;

use clientmail_engine::AccountService;
use clientmail_store::SqliteStore;

use crate::{AccountRecord, CoreError};

/// The entry point Swift holds on to. One instance per database.
///
/// Methods block on disk I/O. Swift must call them off the main actor.
#[derive(Debug, uniffi::Object)]
pub struct CoreClient {
    accounts: AccountService<SqliteStore>,
}

#[uniffi::export]
impl CoreClient {
    /// Opens or creates the database at `database_path`.
    ///
    /// # Arguments
    ///
    /// * `database_path` - Absolute path of the `SQLite` file. Its folder must exist.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Storage`] when the database cannot be opened or migrated.
    #[uniffi::constructor]
    pub fn open(database_path: &str) -> Result<Arc<Self>, CoreError> {
        let store = SqliteStore::open(Path::new(database_path))?;
        Ok(Arc::new(Self::with_store(store)))
    }

    /// Opens a private in-memory database, for tests and `SwiftUI` previews.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Storage`] when the database cannot be created.
    #[uniffi::constructor]
    pub fn open_in_memory() -> Result<Arc<Self>, CoreError> {
        let store = SqliteStore::open_in_memory()?;
        Ok(Arc::new(Self::with_store(store)))
    }

    /// Returns every account, oldest first.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Storage`] when the read fails.
    pub fn accounts(&self) -> Result<Vec<AccountRecord>, CoreError> {
        Ok(self
            .accounts
            .accounts()?
            .iter()
            .map(AccountRecord::from)
            .collect())
    }

    /// Validates and stores a new account.
    ///
    /// # Arguments
    ///
    /// * `address` - The email address as typed.
    /// * `display_name` - The sender name, if any.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidAddress`], [`CoreError::DuplicateAccount`] or
    /// [`CoreError::Storage`].
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI passes optional strings across the boundary as owned values"
    )]
    pub fn add_account(
        &self,
        address: &str,
        display_name: Option<String>,
    ) -> Result<AccountRecord, CoreError> {
        let account = self
            .accounts
            .add_account(address, display_name.as_deref())?;
        Ok(AccountRecord::from(&account))
    }

    /// Removes an account.
    ///
    /// # Arguments
    ///
    /// * `id` - The id from an [`AccountRecord`].
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidAccountId`], [`CoreError::AccountNotFound`] or
    /// [`CoreError::Storage`].
    pub fn remove_account(&self, id: &str) -> Result<(), CoreError> {
        let id = id.parse().map_err(CoreError::from)?;
        self.accounts.remove_account(id)?;
        Ok(())
    }
}

impl CoreClient {
    const fn with_store(store: SqliteStore) -> Self {
        Self {
            accounts: AccountService::new(store),
        }
    }
}
