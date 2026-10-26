use rusqlite::Connection;

use crate::StoreError;

/// Schema changes, applied in order. The first entry moves the database to `user_version` 1.
///
/// Rules for a new migration:
/// - Never edit an entry that has shipped: append a new one instead.
/// - A migration may rebuild a table (create the new one, copy, drop the old one, rename). It runs
///   with foreign keys off, so dropping the old table does not cascade into child rows; it must
///   leave every foreign key valid, which [`migrate`] checks before committing.
///
/// `include_str!` embeds each file in the binary at compile time, so no SQL is read from disk at
/// runtime.
const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001_accounts.sql"),
    include_str!("../migrations/0002_mailboxes_messages.sql"),
];

/// Brings the schema up to date.
///
/// # Arguments
///
/// * `connection` - An open connection. Mutable because transactions borrow it exclusively.
///
/// # Errors
///
/// Returns [`StoreError::Sqlite`] when a statement fails, or
/// [`StoreError::ForeignKeyViolation`] when a migration leaves a dangling reference. The failing
/// migration is rolled back; earlier ones stay applied.
pub(crate) fn migrate(connection: &mut Connection) -> Result<(), StoreError> {
    migrate_with(connection, MIGRATIONS)
}

/// Applies the pending entries of `migrations`. Separate from [`migrate`] so tests can run their
/// own lists.
fn migrate_with(connection: &mut Connection, migrations: &[&str]) -> Result<(), StoreError> {
    let current = user_version(connection)?;
    // `(1_u32..).zip(..)` numbers the migrations from 1 without any integer casts.
    let pending = (1_u32..)
        .zip(migrations)
        .filter(|(version, _)| *version > current);
    for (version, sql) in pending {
        // `PRAGMA foreign_keys` has no effect inside a transaction, so it is switched outside.
        connection.pragma_update(None, "foreign_keys", false)?;
        let applied = apply(connection, version, sql);
        connection.pragma_update(None, "foreign_keys", true)?;
        applied?;
    }
    Ok(())
}

/// Runs one migration and its `user_version` bump in a transaction, so a crash never leaves a
/// half-applied step.
fn apply(connection: &mut Connection, version: u32, sql: &str) -> Result<(), StoreError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(sql)?;
    let violations: i64 =
        transaction.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })?;
    if violations > 0 {
        // Returning drops `transaction` without `commit`, which rolls it back.
        return Err(StoreError::ForeignKeyViolation);
    }
    transaction.pragma_update(None, "user_version", version)?;
    transaction.commit()?;
    Ok(())
}

fn user_version(connection: &Connection) -> Result<u32, StoreError> {
    Ok(connection.pragma_query_value(None, "user_version", |row| row.get(0))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PARENT_AND_CHILD: &str = "
        CREATE TABLE parent (id TEXT PRIMARY KEY NOT NULL, kind TEXT NOT NULL) STRICT;
        CREATE TABLE child (
            parent_id TEXT NOT NULL REFERENCES parent(id) ON DELETE CASCADE
        ) STRICT;";
    const REBUILD_PARENT: &str = "
        CREATE TABLE parent_new (id TEXT PRIMARY KEY NOT NULL, kind TEXT NOT NULL,
                                 note TEXT) STRICT;
        INSERT INTO parent_new (id, kind) SELECT id, kind FROM parent;
        DROP TABLE parent;
        ALTER TABLE parent_new RENAME TO parent;";
    const DANGLING_CHILD: &str = "INSERT INTO child (parent_id) VALUES ('missing');";

    fn latest_version() -> u32 {
        u32::try_from(MIGRATIONS.len()).unwrap()
    }

    fn count(connection: &Connection, table: &str) -> i64 {
        connection
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap()
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

    #[test]
    fn version_one_database_migrates_and_keeps_its_accounts() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrate_with(&mut connection, &MIGRATIONS[..1]).unwrap();
        connection
            .execute(
                "INSERT INTO accounts (id, address, provider) VALUES ('a', 'x@y.z', 'gmail')",
                [],
            )
            .unwrap();

        migrate(&mut connection).unwrap();

        assert_eq!(
            user_version(&connection).unwrap(),
            latest_version(),
            "migrated"
        );
        assert_eq!(count(&connection, "accounts"), 1, "existing account kept");
    }

    #[test]
    fn rebuilding_a_parent_table_keeps_its_child_rows() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        migrate_with(&mut connection, &[PARENT_AND_CHILD]).unwrap();
        connection
            .execute_batch("INSERT INTO parent VALUES ('p', 'x'); INSERT INTO child VALUES ('p');")
            .unwrap();

        migrate_with(&mut connection, &[PARENT_AND_CHILD, REBUILD_PARENT]).unwrap();
        let foreign_keys: bool = connection
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .unwrap();

        assert_eq!(
            count(&connection, "child"),
            1,
            "child row survived the rebuild"
        );
        assert!(foreign_keys, "foreign keys are on again");
    }

    #[test]
    fn migration_that_breaks_a_foreign_key_is_rolled_back() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrate_with(&mut connection, &[PARENT_AND_CHILD]).unwrap();

        let result = migrate_with(&mut connection, &[PARENT_AND_CHILD, DANGLING_CHILD]);

        assert!(
            matches!(result, Err(StoreError::ForeignKeyViolation)),
            "violation reported, got {result:?}"
        );
        assert_eq!(user_version(&connection).unwrap(), 1, "version not bumped");
        assert_eq!(count(&connection, "child"), 0, "insert rolled back");
    }
}
