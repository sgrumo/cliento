use std::path::Path;
use std::sync::Arc;

use clientmail_engine::AccountService;
use clientmail_store::SqliteStore;

use crate::runtime::CoreRuntime;
use crate::secrets::ForeignSecretStore;
use crate::{AccountRecord, CoreError, SecretStoreBridge};

type Accounts = AccountService<SqliteStore, ForeignSecretStore>;

// Swift calls the client from any thread, and each call moves the service to the core's threads.
// This fails to compile if any of these types stops being `Send + Sync` (`core/AGENTS.md`).
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<CoreClient>();
    assert_send_sync::<CoreRuntime>();
    assert_send_sync::<Accounts>();
};

/// The entry point Swift holds on to. One instance per database.
///
/// The constructors block on disk I/O; Swift calls them off the main actor, except at launch (see
/// `apps/macos/AGENTS.md`). Every other method is `async` and runs its work on the client's own
/// threads.
#[derive(Debug, uniffi::Object)]
pub struct CoreClient {
    runtime: CoreRuntime,
    // Work runs on other threads and must own what it uses, so each call clones this `Arc` (a
    // reference count, not the service) into its closure.
    accounts: Arc<Accounts>,
}

#[uniffi::export]
impl CoreClient {
    /// Opens or creates the database at `database_path`.
    ///
    /// # Arguments
    ///
    /// * `database_path` - Absolute path of the `SQLite` file. Its folder must exist.
    /// * `secrets` - The platform's secret storage (the Keychain on macOS).
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Storage`] when the database cannot be opened or migrated, and
    /// [`CoreError::Runtime`] when the core cannot start its threads.
    #[uniffi::constructor]
    pub fn open(
        database_path: &str,
        secrets: Arc<dyn SecretStoreBridge>,
    ) -> Result<Arc<Self>, CoreError> {
        let store = SqliteStore::open(Path::new(database_path))?;
        Ok(Arc::new(Self::with_stores(store, secrets)?))
    }

    /// Opens a private in-memory database, for tests and `SwiftUI` previews.
    ///
    /// # Arguments
    ///
    /// * `secrets` - The secret storage to use; previews pass an in-memory one.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Storage`] when the database cannot be created, and
    /// [`CoreError::Runtime`] when the core cannot start its threads.
    #[uniffi::constructor]
    pub fn open_in_memory(secrets: Arc<dyn SecretStoreBridge>) -> Result<Arc<Self>, CoreError> {
        let store = SqliteStore::open_in_memory()?;
        Ok(Arc::new(Self::with_stores(store, secrets)?))
    }

    /// Returns every account, oldest first.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Storage`] when the read fails.
    pub async fn accounts(&self) -> Result<Vec<AccountRecord>, CoreError> {
        let accounts = Arc::clone(&self.accounts);
        let listed = self.runtime.blocking(move || accounts.accounts()).await?;
        Ok(listed.iter().map(AccountRecord::from).collect())
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
    pub async fn add_account(
        &self,
        address: String,
        display_name: Option<String>,
    ) -> Result<AccountRecord, CoreError> {
        let accounts = Arc::clone(&self.accounts);
        let account = self
            .runtime
            .blocking(move || accounts.add_account(&address, display_name.as_deref()))
            .await?;
        Ok(AccountRecord::from(&account))
    }

    /// Removes an account and its secrets.
    ///
    /// # Arguments
    ///
    /// * `id` - The id from an [`AccountRecord`].
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidAccountId`], [`CoreError::AccountNotFound`],
    /// [`CoreError::SecretStorageUnavailable`] or [`CoreError::SecretStorage`] (in both cases the
    /// account is kept), or [`CoreError::Storage`].
    pub async fn remove_account(&self, id: String) -> Result<(), CoreError> {
        let id = id.parse().map_err(CoreError::from)?;
        let accounts = Arc::clone(&self.accounts);
        self.runtime
            .blocking(move || accounts.remove_account(id))
            .await?;
        Ok(())
    }
}

impl CoreClient {
    fn with_stores(
        store: SqliteStore,
        secrets: Arc<dyn SecretStoreBridge>,
    ) -> Result<Self, CoreError> {
        Ok(Self {
            runtime: CoreRuntime::new()?,
            accounts: Arc::new(AccountService::new(store, ForeignSecretStore::new(secrets))),
        })
    }
}
