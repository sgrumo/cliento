//! End-to-end tests of the account API through the real engine and `SQLite` store.

use clientmail_ffi::{CoreClient, CoreError, ProviderKind, core_version};

#[test]
fn add_list_and_remove_account() {
    let client = CoreClient::open_in_memory().unwrap();

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
fn duplicate_and_invalid_input_map_to_core_errors() {
    let client = CoreClient::open_in_memory().unwrap();
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
    let folder = std::env::temp_dir().join(format!("clientmail-ffi-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join("mail.sqlite");
    let path = path.to_str().unwrap();

    let added = CoreClient::open(path)
        .unwrap()
        .add_account("ada@example.com", None)
        .unwrap();
    let reopened = CoreClient::open(path).unwrap().accounts().unwrap();

    assert_eq!(reopened, vec![added], "account survives reopen");
    std::fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn core_version_matches_crate_version() {
    assert_eq!(core_version(), env!("CARGO_PKG_VERSION"), "version");
}
