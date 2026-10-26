//! End-to-end tests of the account API through the real engine and `SQLite` store, with a Rust
//! implementation of the secret bridge standing in for the Keychain.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use clientmail_ffi::{
    CoreClient, CoreError, ProviderKind, SecretBridgeError, SecretStoreBridge, core_version,
};

/// Secret bridge backed by a map, as the Swift in-memory store is.
#[derive(Default)]
struct MemorySecrets {
    items: Mutex<HashMap<String, String>>,
}

impl SecretStoreBridge for MemorySecrets {
    fn read(&self, label: String) -> Result<Option<String>, SecretBridgeError> {
        Ok(self.items()?.get(&label).cloned())
    }

    fn write(&self, label: String, secret: String) -> Result<(), SecretBridgeError> {
        self.items()?.insert(label, secret);
        Ok(())
    }

    fn delete(&self, label: String) -> Result<bool, SecretBridgeError> {
        Ok(self.items()?.remove(&label).is_some())
    }
}

impl MemorySecrets {
    /// Locks the map. A poisoned lock (a panic in another test thread) reads as unavailable.
    fn items(&self) -> Result<MutexGuard<'_, HashMap<String, String>>, SecretBridgeError> {
        self.items
            .lock()
            .map_err(|_| SecretBridgeError::Unavailable)
    }
}

/// Secret bridge that is always locked.
struct LockedSecrets;

impl SecretStoreBridge for LockedSecrets {
    fn read(&self, _label: String) -> Result<Option<String>, SecretBridgeError> {
        Err(SecretBridgeError::Unavailable)
    }

    fn write(&self, _label: String, _secret: String) -> Result<(), SecretBridgeError> {
        Err(SecretBridgeError::Unavailable)
    }

    fn delete(&self, _label: String) -> Result<bool, SecretBridgeError> {
        Err(SecretBridgeError::Unavailable)
    }
}

fn client() -> Result<Arc<CoreClient>, CoreError> {
    CoreClient::open_in_memory(Arc::new(MemorySecrets::default()))
}

#[test]
fn add_list_and_remove_account() {
    let client = client().unwrap();

    let added = client
        .add_account("  Ada@Gmail.com ", Some("Ada".to_owned()))
        .unwrap();
    let listed = client.accounts().unwrap();
    client.remove_account(&added.id).unwrap();

    assert_eq!(added.address, "Ada@gmail.com", "address normalised");
    assert_eq!(added.provider, ProviderKind::Gmail, "provider detected");
    assert_eq!(listed, vec![added], "account listed");
    assert!(client.accounts().unwrap().is_empty(), "account removed");
}

#[test]
fn removing_an_account_deletes_its_secret() {
    let secrets = Arc::new(MemorySecrets::default());
    let client = CoreClient::open_in_memory(Arc::<MemorySecrets>::clone(&secrets)).unwrap();
    let added = client.add_account("ada@example.com", None).unwrap();
    let label = format!("{}:oauth-refresh-token", added.id);
    secrets
        .items
        .lock()
        .unwrap()
        .insert(label.clone(), "token".to_owned());

    client.remove_account(&added.id).unwrap();

    assert!(
        !secrets.items.lock().unwrap().contains_key(&label),
        "token deleted with the account"
    );
}

#[test]
fn locked_secret_storage_keeps_the_account() {
    let client = CoreClient::open_in_memory(Arc::new(LockedSecrets)).unwrap();
    let added = client.add_account("ada@example.com", None).unwrap();

    let result = client.remove_account(&added.id);

    assert_eq!(
        result,
        Err(CoreError::SecretStorageUnavailable),
        "locked is reported as retryable"
    );
    assert_eq!(client.accounts().unwrap(), vec![added], "account kept");
}

#[test]
fn duplicate_and_invalid_input_map_to_core_errors() {
    let client = client().unwrap();
    client.add_account("ada@example.com", None).unwrap();

    let duplicate = client.add_account("ada@example.com", None);
    let invalid = client.add_account("nope", None);
    let bad_id = client.remove_account("not-a-uuid");

    assert_eq!(duplicate, Err(CoreError::DuplicateAccount), "duplicate");
    assert_eq!(invalid, Err(CoreError::InvalidAddress), "invalid address");
    assert_eq!(bad_id, Err(CoreError::InvalidAccountId), "invalid id");
}

#[test]
fn accounts_persist_across_reopen() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("mail.sqlite");
    let path = path.to_str().unwrap();
    let secrets = Arc::new(MemorySecrets::default());

    let added = CoreClient::open(path, Arc::<MemorySecrets>::clone(&secrets))
        .unwrap()
        .add_account("ada@example.com", None)
        .unwrap();
    let reopened = CoreClient::open(path, secrets).unwrap().accounts().unwrap();

    assert_eq!(reopened, vec![added], "account survives reopen");
}

#[test]
fn core_version_matches_crate_version() {
    assert_eq!(core_version(), env!("CARGO_PKG_VERSION"), "version");
}
