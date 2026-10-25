use clientmail_domain::ports::AccountRepository;
use clientmail_domain::{Account, AccountId, EmailAddress, Provider};
use rusqlite::{OptionalExtension, Row, params};

use crate::{SqliteStore, StoreError};

const SELECT_COLUMNS: &str = "SELECT id, address, display_name, provider FROM accounts";

impl AccountRepository for SqliteStore {
    type Error = StoreError;

    fn list(&self) -> Result<Vec<Account>, StoreError> {
        self.with_connection(|connection| {
            let mut statement =
                connection.prepare_cached(&format!("{SELECT_COLUMNS} ORDER BY id"))?;
            let rows = statement.query_map([], read_row)?;
            // Each row is a `Result` twice over: `SQLite` can fail, and so can domain parsing.
            // `collect` into `Result<Vec<_>, _>` stops at the first error.
            rows.map(|row| row?).collect()
        })
    }

    fn find_by_address(&self, address: &EmailAddress) -> Result<Option<Account>, StoreError> {
        self.with_connection(|connection| {
            let mut statement =
                connection.prepare_cached(&format!("{SELECT_COLUMNS} WHERE address = ?1"))?;
            statement
                .query_row([address.as_str()], read_row)
                .optional()?
                .transpose()
        })
    }

    fn insert(&self, account: &Account) -> Result<(), StoreError> {
        self.with_connection(|connection| {
            connection.execute(
                "INSERT INTO accounts (id, address, display_name, provider) \
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    account.id().to_string(),
                    account.address().as_str(),
                    account.display_name(),
                    account.provider().as_str(),
                ],
            )?;
            Ok(())
        })
    }

    fn delete(&self, id: AccountId) -> Result<bool, StoreError> {
        self.with_connection(|connection| {
            let deleted =
                connection.execute("DELETE FROM accounts WHERE id = ?1", [id.to_string()])?;
            Ok(deleted > 0)
        })
    }
}

/// Maps a row to an account. The outer `Result` is `SQLite`'s, the inner one is domain parsing.
fn read_row(row: &Row<'_>) -> rusqlite::Result<Result<Account, StoreError>> {
    let id: String = row.get(0)?;
    let address: String = row.get(1)?;
    let display_name: Option<String> = row.get(2)?;
    let provider: String = row.get(3)?;
    Ok(build_account(&id, &address, display_name, &provider))
}

fn build_account(
    id: &str,
    address: &str,
    display_name: Option<String>,
    provider: &str,
) -> Result<Account, StoreError> {
    let id = id
        .parse()
        .map_err(|_| StoreError::CorruptRow { column: "id" })?;
    let address =
        EmailAddress::parse(address).map_err(|_| StoreError::CorruptRow { column: "address" })?;
    let provider =
        Provider::from_identifier(provider).ok_or(StoreError::CorruptRow { column: "provider" })?;
    Ok(Account::from_parts(id, address, display_name, provider))
}

#[cfg(test)]
mod tests {
    use clientmail_domain::NewAccount;

    use super::*;

    fn account(address: &str) -> Account {
        NewAccount::parse(address, Some("Test"))
            .unwrap()
            .into_account()
    }

    #[test]
    fn insert_then_list_returns_accounts_oldest_first() {
        let store = SqliteStore::open_in_memory().unwrap();
        let first = account("first@example.com");
        let second = account("second@gmail.com");

        store.insert(&first).unwrap();
        store.insert(&second).unwrap();
        let listed = store.list().unwrap();

        assert_eq!(listed, vec![first, second], "accounts in creation order");
    }

    #[test]
    fn find_by_address_ignores_case() {
        let store = SqliteStore::open_in_memory().unwrap();
        let stored = account("Ada@example.com");
        store.insert(&stored).unwrap();
        let lookup = EmailAddress::parse("ada@EXAMPLE.com").unwrap();

        let found = store.find_by_address(&lookup).unwrap();

        assert_eq!(found, Some(stored), "lookup must ignore case");
    }

    #[test]
    fn find_by_address_returns_none_when_missing() {
        let store = SqliteStore::open_in_memory().unwrap();
        let lookup = EmailAddress::parse("nobody@example.com").unwrap();

        let found = store.find_by_address(&lookup).unwrap();

        assert_eq!(found, None, "no account stored");
    }

    #[test]
    fn insert_rejects_duplicate_address() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.insert(&account("ada@example.com")).unwrap();

        let result = store.insert(&account("ada@example.com"));

        assert!(
            matches!(result, Err(StoreError::Sqlite(_))),
            "unique constraint must reject duplicates, got {result:?}"
        );
    }

    #[test]
    fn delete_reports_whether_a_row_was_removed() {
        let store = SqliteStore::open_in_memory().unwrap();
        let stored = account("ada@example.com");
        store.insert(&stored).unwrap();

        let first = store.delete(stored.id()).unwrap();
        let second = store.delete(stored.id()).unwrap();

        assert!(first, "first delete removes the row");
        assert!(!second, "second delete finds nothing");
        assert!(store.list().unwrap().is_empty(), "store is empty");
    }

    #[test]
    fn build_account_rejects_unknown_provider() {
        let id = AccountId::generate().to_string();

        let result = build_account(&id, "ada@example.com", None, "aol");

        assert!(
            matches!(result, Err(StoreError::CorruptRow { column: "provider" })),
            "unknown provider must be reported, got {result:?}"
        );
    }
}
