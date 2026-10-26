//! End-to-end tests of the account API through the real engine and `SQLite` store, with a Rust
//! implementation of the secret bridge standing in for the Keychain.
//!
//! The tests await the async API with `pollster`, outside any Tokio context, as Swift does.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;

use pollster::block_on;

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

    let added = block_on(client.add_account("  Ada@Gmail.com ".to_owned(), Some("Ada".to_owned())))
        .unwrap();
    let listed = block_on(client.accounts()).unwrap();
    block_on(client.remove_account(added.id.clone())).unwrap();

    assert_eq!(added.address, "Ada@gmail.com", "address normalised");
    assert_eq!(added.provider, ProviderKind::Gmail, "provider detected");
    assert_eq!(listed, vec![added], "account listed");
    assert!(
        block_on(client.accounts()).unwrap().is_empty(),
        "account removed"
    );
}

#[test]
fn removing_an_account_deletes_its_secret() {
    let secrets = Arc::new(MemorySecrets::default());
    let client = CoreClient::open_in_memory(Arc::<MemorySecrets>::clone(&secrets)).unwrap();
    let added = block_on(client.add_account("ada@example.com".to_owned(), None)).unwrap();
    let label = format!("{}:oauth-refresh-token", added.id);
    secrets
        .items
        .lock()
        .unwrap()
        .insert(label.clone(), "token".to_owned());

    block_on(client.remove_account(added.id)).unwrap();

    assert!(
        !secrets.items.lock().unwrap().contains_key(&label),
        "token deleted with the account"
    );
}

#[test]
fn locked_secret_storage_keeps_the_account() {
    let client = CoreClient::open_in_memory(Arc::new(LockedSecrets)).unwrap();
    let added = block_on(client.add_account("ada@example.com".to_owned(), None)).unwrap();

    let result = block_on(client.remove_account(added.id.clone()));

    assert_eq!(
        result,
        Err(CoreError::SecretStorageUnavailable),
        "locked is reported as retryable"
    );
    assert_eq!(
        block_on(client.accounts()).unwrap(),
        vec![added],
        "account kept"
    );
}

#[test]
fn duplicate_and_invalid_input_map_to_core_errors() {
    let client = client().unwrap();
    block_on(client.add_account("ada@example.com".to_owned(), None)).unwrap();

    let duplicate = block_on(client.add_account("ada@example.com".to_owned(), None));
    let invalid = block_on(client.add_account("nope".to_owned(), None));
    let bad_id = block_on(client.remove_account("not-a-uuid".to_owned()));

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

    let first = CoreClient::open(path, Arc::<MemorySecrets>::clone(&secrets)).unwrap();
    let added = block_on(first.add_account("ada@example.com".to_owned(), None)).unwrap();
    drop(first);
    let reopened = block_on(CoreClient::open(path, secrets).unwrap().accounts()).unwrap();

    assert_eq!(reopened, vec![added], "account survives reopen");
}

/// Secret bridge that records the name of each thread that calls it.
#[derive(Default)]
struct ThreadRecordingSecrets {
    threads: Mutex<Vec<Option<String>>>,
}

impl ThreadRecordingSecrets {
    /// Records the calling thread. A poisoned lock reads as unavailable, as in `MemorySecrets`.
    fn record(&self) -> Result<(), SecretBridgeError> {
        let name = thread::current().name().map(ToOwned::to_owned);
        self.threads
            .lock()
            .map_err(|_| SecretBridgeError::Unavailable)?
            .push(name);
        Ok(())
    }
}

impl SecretStoreBridge for ThreadRecordingSecrets {
    fn read(&self, _label: String) -> Result<Option<String>, SecretBridgeError> {
        self.record()?;
        Ok(None)
    }

    fn write(&self, _label: String, _secret: String) -> Result<(), SecretBridgeError> {
        self.record()
    }

    fn delete(&self, _label: String) -> Result<bool, SecretBridgeError> {
        self.record()?;
        Ok(false)
    }
}

#[test]
fn secret_bridge_runs_on_core_threads() {
    let secrets = Arc::new(ThreadRecordingSecrets::default());
    let client =
        CoreClient::open_in_memory(Arc::<ThreadRecordingSecrets>::clone(&secrets)).unwrap();
    let added = block_on(client.add_account("ada@example.com".to_owned(), None)).unwrap();

    block_on(client.remove_account(added.id)).unwrap();

    let threads = secrets.threads.lock().unwrap().clone();
    assert!(!threads.is_empty(), "the bridge was called");
    assert!(
        threads
            .iter()
            .all(|name| name.as_deref() == Some("clientmail-core")),
        "every call on a core thread: {threads:?}"
    );
}

#[test]
fn concurrent_calls_all_complete() {
    let client = client().unwrap();

    #[expect(
        clippy::needless_collect,
        reason = "collecting starts every thread before the first join, so the calls overlap"
    )]
    let callers: Vec<_> = (0..20)
        .map(|index| {
            let client = Arc::clone(&client);
            thread::spawn(move || {
                block_on(client.add_account(format!("user{index}@example.com"), None))
            })
        })
        .collect();
    let results: Vec<_> = callers
        .into_iter()
        .map(|caller| caller.join().unwrap())
        .collect();

    assert!(results.iter().all(Result::is_ok), "every add succeeded");
    assert_eq!(block_on(client.accounts()).unwrap().len(), 20, "all listed");
}

#[test]
fn core_version_matches_crate_version() {
    assert_eq!(core_version(), env!("CARGO_PKG_VERSION"), "version");
}
