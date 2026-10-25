use rusqlite::Connection;

use crate::StoreError;

/// Schema changes, applied in order. The first entry moves the database to `user_version` 1.
///
/// Never edit an entry that has shipped: append a new one instead.
const MIGRATIONS: &[&str] = &["CREATE TABLE accounts (
        id TEXT PRIMARY KEY NOT NULL,
        address TEXT NOT NULL UNIQUE COLLATE NOCASE,
        display_name TEXT,
        provider TEXT NOT NULL
    ) STRICT;"];

/// Brings the schema up to date.
///
/// Each migration runs in its own transaction together with its `user_version` bump, so a crash
/// never leaves a half-applied step.
///
/// # Arguments
///
/// * `connection` - An open connection. Mutable because transactions borrow it exclusively.
///
/// # Errors
///
/// Returns [`StoreError::Sqlite`] when a statement fails.
pub(crate) fn migrate(connection: &mut Connection) -> Result<(), StoreError> {
    let current = user_version(connection)?;
    // `(1_u32..).zip(..)` numbers the migrations from 1 without any integer casts.
    let pending = (1_u32..)
        .zip(MIGRATIONS)
        .filter(|(version, _)| *version > current);
    for (version, sql) in pending {
        let transaction = connection.transaction()?;
        transaction.execute_batch(sql)?;
        transaction.pragma_update(None, "user_version", version)?;
        transaction.commit()?;
    }
    Ok(())
}

fn user_version(connection: &Connection) -> Result<u32, StoreError> {
    Ok(connection.pragma_query_value(None, "user_version", |row| row.get(0))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn latest_version() -> u32 {
        u32::try_from(MIGRATIONS.len()).unwrap()
    }

    #[test]
    fn migrate_sets_latest_version() {
        let mut connection = Connection::open_in_memory().unwrap();

        migrate(&mut connection).unwrap();

        assert_eq!(
            user_version(&connection).unwrap(),
            latest_version(),
            "latest version"
        );
    }

    #[test]
    fn migrate_is_idempotent() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrate(&mut connection).unwrap();

        migrate(&mut connection).unwrap();

        assert_eq!(
            user_version(&connection).unwrap(),
            latest_version(),
            "unchanged version"
        );
    }
}
