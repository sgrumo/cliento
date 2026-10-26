use clientmail_domain::ports::MailboxRepository;
use clientmail_domain::{
    AccountId, ImapUid, Mailbox, MailboxId, MailboxPath, MailboxSyncState, ModSeq, UidValidity,
};
use rusqlite::{OptionalExtension, Row, params};

use crate::mapping::{
    role_from_column, role_to_column, u32_from_column, u64_from_column, u64_to_column,
};
use crate::{SqliteStore, StoreError};

impl MailboxRepository for SqliteStore {
    type Error = StoreError;

    fn mailboxes(&self, account: AccountId) -> Result<Vec<Mailbox>, StoreError> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare_cached(
                "SELECT id, account_id, path, delimiter, role, selectable FROM mailboxes \
                 WHERE account_id = ?1 ORDER BY path",
            )?;
            let rows = statement.query_map([account.to_string()], read_mailbox_row)?;
            rows.map(|row| row?).collect()
        })
    }

    fn upsert_mailbox(&self, mailbox: &Mailbox) -> Result<(), StoreError> {
        self.with_connection(|connection| {
            connection.execute(
                "INSERT INTO mailboxes (id, account_id, path, delimiter, role, selectable) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
                 ON CONFLICT (id) DO UPDATE SET path = excluded.path, \
                 delimiter = excluded.delimiter, role = excluded.role, \
                 selectable = excluded.selectable",
                params![
                    mailbox.id().to_string(),
                    mailbox.account_id().to_string(),
                    mailbox.path().as_str(),
                    mailbox.path().delimiter().map(String::from),
                    role_to_column(mailbox.role()),
                    mailbox.is_selectable(),
                ],
            )?;
            Ok(())
        })
    }

    fn delete_mailbox(&self, id: MailboxId) -> Result<bool, StoreError> {
        self.with_connection(|connection| {
            let deleted =
                connection.execute("DELETE FROM mailboxes WHERE id = ?1", [id.to_string()])?;
            Ok(deleted > 0)
        })
    }

    fn sync_state(&self, id: MailboxId) -> Result<Option<MailboxSyncState>, StoreError> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare_cached(
                "SELECT uid_validity, uid_next, highest_modseq FROM mailboxes WHERE id = ?1",
            )?;
            let columns: Option<(Option<i64>, Option<i64>, Option<i64>)> = statement
                .query_row([id.to_string()], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })
                .optional()?;
            // A missing mailbox and a never-synced one (no `UIDVALIDITY` yet) both mean "no
            // state"; `and_then` merges the two `Option` layers.
            columns
                .and_then(|(uid_validity, uid_next, modseq)| {
                    uid_validity
                        .map(|uid_validity| build_sync_state(uid_validity, uid_next, modseq))
                })
                .transpose()
        })
    }

    fn set_sync_state(&self, id: MailboxId, state: &MailboxSyncState) -> Result<(), StoreError> {
        self.with_connection(|connection| {
            let updated = connection.execute(
                "UPDATE mailboxes SET uid_validity = ?2, uid_next = ?3, highest_modseq = ?4 \
                 WHERE id = ?1",
                params![
                    id.to_string(),
                    state.uid_validity().get(),
                    state.uid_next().map(ImapUid::get),
                    state
                        .highest_modseq()
                        .map(|modseq| u64_to_column(modseq.get())),
                ],
            )?;
            if updated == 0 {
                return Err(StoreError::MissingMailbox);
            }
            Ok(())
        })
    }
}

/// Maps a `mailboxes` row. The outer `Result` is `SQLite`'s, the inner one is domain parsing.
fn read_mailbox_row(row: &Row<'_>) -> rusqlite::Result<Result<Mailbox, StoreError>> {
    let id: String = row.get(0)?;
    let account_id: String = row.get(1)?;
    let path: String = row.get(2)?;
    let delimiter: Option<String> = row.get(3)?;
    let role: String = row.get(4)?;
    let selectable: bool = row.get(5)?;
    Ok(build_mailbox(MailboxColumns {
        id: &id,
        account_id: &account_id,
        path: &path,
        delimiter: delimiter.as_deref(),
        role: &role,
        selectable,
    }))
}

/// The columns of a `mailboxes` row, with text borrowed from the row's owned strings.
#[derive(Clone, Copy)]
struct MailboxColumns<'row> {
    id: &'row str,
    account_id: &'row str,
    path: &'row str,
    delimiter: Option<&'row str>,
    role: &'row str,
    selectable: bool,
}

fn build_mailbox(columns: MailboxColumns<'_>) -> Result<Mailbox, StoreError> {
    let corrupt = |column| move |_| StoreError::CorruptRow { column };
    let id = columns.id.parse().map_err(corrupt("id"))?;
    let account_id = columns.account_id.parse().map_err(corrupt("account_id"))?;
    let delimiter = columns
        .delimiter
        .map(|text| {
            let mut characters = text.chars();
            match (characters.next(), characters.next()) {
                (Some(delimiter), None) => Ok(delimiter),
                _ => Err(StoreError::CorruptRow {
                    column: "delimiter",
                }),
            }
        })
        .transpose()?;
    let path = MailboxPath::new(columns.path, delimiter).map_err(corrupt("path"))?;
    let role = role_from_column(columns.role)?;
    Ok(Mailbox::new(id, account_id, path, role, columns.selectable))
}

fn build_sync_state(
    uid_validity: i64,
    uid_next: Option<i64>,
    highest_modseq: Option<i64>,
) -> Result<MailboxSyncState, StoreError> {
    let uid_validity = UidValidity::new(u32_from_column(uid_validity, "uid_validity")?).ok_or(
        StoreError::CorruptRow {
            column: "uid_validity",
        },
    )?;
    let uid_next = uid_next
        .map(|value| {
            ImapUid::new(u32_from_column(value, "uid_next")?)
                .ok_or(StoreError::CorruptRow { column: "uid_next" })
        })
        .transpose()?;
    let highest_modseq = highest_modseq
        .map(|value| {
            ModSeq::new(u64_from_column(value)).ok_or(StoreError::CorruptRow {
                column: "highest_modseq",
            })
        })
        .transpose()?;
    Ok(MailboxSyncState::new(
        uid_validity,
        uid_next,
        highest_modseq,
    ))
}

#[cfg(test)]
mod tests {
    use clientmail_domain::ports::AccountRepository;
    use clientmail_domain::{MailboxRole, NewAccount};

    use super::*;

    fn store_with_account() -> (SqliteStore, AccountId) {
        let store = SqliteStore::open_in_memory().unwrap();
        let account = NewAccount::parse("ada@example.com", None)
            .unwrap()
            .into_account();
        store.insert(&account).unwrap();
        (store, account.id())
    }

    fn mailbox(account: AccountId, path: &str, role: MailboxRole) -> Mailbox {
        Mailbox::new(
            MailboxId::generate(),
            account,
            MailboxPath::new(path, Some('/')).unwrap(),
            role,
            true,
        )
    }

    #[test]
    fn upsert_then_list_returns_mailboxes_by_path() {
        let (store, account) = store_with_account();
        let work = mailbox(account, "Work", MailboxRole::Custom);
        let inbox = mailbox(account, "INBOX", MailboxRole::Inbox);

        store.upsert_mailbox(&work).unwrap();
        store.upsert_mailbox(&inbox).unwrap();
        let listed = store.mailboxes(account).unwrap();

        assert_eq!(listed, vec![inbox, work], "ordered by path");
    }

    #[test]
    fn upsert_with_same_id_renames_in_place() {
        let (store, account) = store_with_account();
        let original = mailbox(account, "Clients", MailboxRole::Custom);
        store.upsert_mailbox(&original).unwrap();
        let renamed = Mailbox::new(
            original.id(),
            account,
            MailboxPath::new("Customers", Some('/')).unwrap(),
            MailboxRole::Custom,
            true,
        );

        store.upsert_mailbox(&renamed).unwrap();
        let listed = store.mailboxes(account).unwrap();

        assert_eq!(listed, vec![renamed], "one row, new path, same id");
    }

    #[test]
    fn upsert_rejects_a_second_mailbox_with_the_same_path() {
        let (store, account) = store_with_account();
        store
            .upsert_mailbox(&mailbox(account, "Work", MailboxRole::Custom))
            .unwrap();

        let result = store.upsert_mailbox(&mailbox(account, "Work", MailboxRole::Custom));

        assert!(
            matches!(result, Err(StoreError::Sqlite(_))),
            "unique path per account, got {result:?}"
        );
    }

    #[test]
    fn delete_reports_whether_a_mailbox_was_removed() {
        let (store, account) = store_with_account();
        let work = mailbox(account, "Work", MailboxRole::Custom);
        store.upsert_mailbox(&work).unwrap();

        let first = store.delete_mailbox(work.id()).unwrap();
        let second = store.delete_mailbox(work.id()).unwrap();

        assert!(first, "deleted");
        assert!(!second, "already gone");
        assert!(store.mailboxes(account).unwrap().is_empty(), "no mailboxes");
    }

    #[test]
    fn sync_state_round_trips_with_and_without_optional_values() {
        let (store, account) = store_with_account();
        let inbox = mailbox(account, "INBOX", MailboxRole::Inbox);
        store.upsert_mailbox(&inbox).unwrap();
        let full = MailboxSyncState::new(
            UidValidity::new(u32::MAX).unwrap(),
            ImapUid::new(42),
            ModSeq::new(i64::MAX.cast_unsigned()),
        );
        let bare = MailboxSyncState::new(UidValidity::new(1).unwrap(), None, None);

        let before = store.sync_state(inbox.id()).unwrap();
        store.set_sync_state(inbox.id(), &full).unwrap();
        let after_full = store.sync_state(inbox.id()).unwrap();
        store.set_sync_state(inbox.id(), &bare).unwrap();
        let after_bare = store.sync_state(inbox.id()).unwrap();

        assert_eq!(before, None, "never synced");
        assert_eq!(after_full, Some(full), "every value kept");
        assert_eq!(after_bare, Some(bare), "absent values cleared");
    }

    #[test]
    fn set_sync_state_reports_a_missing_mailbox() {
        let (store, _) = store_with_account();
        let state = MailboxSyncState::new(UidValidity::new(1).unwrap(), None, None);

        let result = store.set_sync_state(MailboxId::generate(), &state);

        assert!(
            matches!(result, Err(StoreError::MissingMailbox)),
            "missing mailbox, got {result:?}"
        );
    }

    #[test]
    fn deleting_the_account_removes_its_mailboxes() {
        let (store, account) = store_with_account();
        store
            .upsert_mailbox(&mailbox(account, "INBOX", MailboxRole::Inbox))
            .unwrap();

        store.delete(account).unwrap();

        assert!(store.mailboxes(account).unwrap().is_empty(), "cascaded");
    }
}
