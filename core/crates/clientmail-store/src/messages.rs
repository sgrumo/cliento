use clientmail_domain::ports::{MessageRepository, PageCursor, PageQuery};
use clientmail_domain::{HeaderMessageId, MailboxId, Message, MessageId, UnixTime};
use rusqlite::{Connection, OptionalExtension, params};

use crate::mapping::{DELETED_COLUMN_BIT, flags_to_column};
use crate::message_rows::{
    MESSAGE_COLUMNS, MessageChildren, MessageRow, ParticipantField, RemoteColumns, build_message,
};
use crate::{SqliteStore, StoreError};

/// Header and flag columns every upsert overwrites when the message is already stored.
const UPDATED_COLUMNS: &str = "subject = excluded.subject, sent_at = excluded.sent_at, \
    received_at = excluded.received_at, header_message_id = excluded.header_message_id, \
    in_reply_to = excluded.in_reply_to, list_id = excluded.list_id, \
    has_list_unsubscribe = excluded.has_list_unsubscribe, size_bytes = excluded.size_bytes, \
    flags = excluded.flags";

/// Server-key columns, overwritten when a stored message is matched by its id.
const REMOTE_COLUMNS: &str = "remote_kind = excluded.remote_kind, \
    mailbox_id = excluded.mailbox_id, uid_validity = excluded.uid_validity, \
    uid = excluded.uid, gmail_message_id = excluded.gmail_message_id, \
    gmail_thread_id = excluded.gmail_thread_id";

impl MessageRepository for SqliteStore {
    type Error = StoreError;

    fn upsert_messages(&self, messages: &[Message]) -> Result<Vec<MessageId>, StoreError> {
        self.with_connection(|connection| {
            // `unchecked_transaction` works through `&Connection`; the store's `Mutex` already
            // guarantees no other transaction runs at the same time.
            let transaction = connection.unchecked_transaction()?;
            let ids = messages
                .iter()
                .map(|message| upsert_message(&transaction, message))
                .collect::<Result<Vec<_>, _>>()?;
            transaction.commit()?;
            Ok(ids)
        })
    }

    fn set_labels(&self, id: MessageId, labels: &[MailboxId]) -> Result<(), StoreError> {
        let id = id.to_string();
        self.with_connection(|connection| {
            let kind: Option<String> = connection
                .prepare_cached("SELECT remote_kind FROM messages WHERE id = ?1")?
                .query_row([&id], |row| row.get(0))
                .optional()?;
            match kind.as_deref() {
                None => return Err(StoreError::MissingMessage),
                Some("imap") => return Err(StoreError::LabelsOnImapMessage),
                Some(_) => {}
            }
            let transaction = connection.unchecked_transaction()?;
            transaction.execute("DELETE FROM message_labels WHERE message_id = ?1", [&id])?;
            let mut insert = transaction.prepare_cached(
                "INSERT INTO message_labels (message_id, mailbox_id, received_at) \
                 SELECT id, ?2, received_at FROM messages WHERE id = ?1",
            )?;
            for label in labels {
                insert.execute([&id, &label.to_string()])?;
            }
            drop(insert);
            transaction.commit()?;
            Ok(())
        })
    }

    fn message(&self, id: MessageId) -> Result<Option<Message>, StoreError> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare_cached(&format!(
                "SELECT {MESSAGE_COLUMNS} FROM messages m WHERE m.id = ?1"
            ))?;
            let row = statement
                .query_row([id.to_string()], MessageRow::read)
                .optional()?;
            row.map(|row| load_message(connection, &row)).transpose()
        })
    }

    fn page(&self, query: &PageQuery) -> Result<Vec<Message>, StoreError> {
        self.with_connection(|connection| {
            // A mailbox is a plain IMAP folder or a Gmail label. Each source is read with its own
            // index, newest first, and at most `limit` rows; the two sorted lists are merged.
            // In practice one of them is empty.
            let mut rows = page_rows(connection, PageSource::Folder, query)?;
            rows.extend(page_rows(connection, PageSource::Label, query)?);
            rows.sort_by(|left, right| right.list_key().cmp(&left.list_key()));
            rows.truncate(usize::try_from(query.limit).unwrap_or(usize::MAX));
            rows.iter()
                .map(|row| load_message(connection, row))
                .collect()
        })
    }
}

/// Where a page reads its rows from.
#[derive(Clone, Copy)]
enum PageSource {
    /// Plain IMAP messages, by `messages.mailbox_id`.
    Folder,
    /// Gmail messages, by `message_labels`.
    Label,
}

/// Reads up to `query.limit` rows of one source, newest first, after the cursor if any.
///
/// The first page and later pages use different statements: with the cursor written as
/// `?2 IS NULL OR ...`, `SQLite` cannot use the index to jump to the cursor.
fn page_rows(
    connection: &Connection,
    source: PageSource,
    query: &PageQuery,
) -> Result<Vec<MessageRow>, StoreError> {
    let (from, order_time, order_id) = match source {
        PageSource::Folder => (
            "messages m WHERE m.mailbox_id = ?1",
            "m.received_at",
            "m.id",
        ),
        PageSource::Label => (
            "message_labels l JOIN messages m ON m.id = l.message_id WHERE l.mailbox_id = ?1",
            "l.received_at",
            "l.message_id",
        ),
    };
    let not_deleted = format!("(m.flags & {DELETED_COLUMN_BIT}) = 0");
    let order = format!("ORDER BY {order_time} DESC, {order_id} DESC");
    let rows = match query.before {
        None => connection
            .prepare_cached(&format!(
                "SELECT {MESSAGE_COLUMNS} FROM {from} AND {not_deleted} {order} LIMIT ?2"
            ))?
            .query_map(
                params![query.mailbox.to_string(), query.limit],
                MessageRow::read,
            )?
            .collect::<Result<Vec<_>, _>>()?,
        Some(PageCursor { received_at, id }) => connection
            .prepare_cached(&format!(
                "SELECT {MESSAGE_COLUMNS} FROM {from} AND {not_deleted} \
                 AND ({order_time}, {order_id}) < (?2, ?3) {order} LIMIT ?4"
            ))?
            .query_map(
                params![
                    query.mailbox.to_string(),
                    received_at.seconds(),
                    id.to_string(),
                    query.limit,
                ],
                MessageRow::read,
            )?
            .collect::<Result<Vec<_>, _>>()?,
    };
    Ok(rows)
}

/// Upserts one message and replaces its child rows. Returns the stored id.
///
/// A stored message is found by id first, so a caller that re-matched a message can give it a
/// new server key (`UIDVALIDITY` change, move to another folder) and keep its id. Otherwise it is
/// found by its server key, and keeps the id it was stored with.
fn upsert_message(connection: &Connection, message: &Message) -> Result<MessageId, StoreError> {
    let remote = RemoteColumns::of(message.remote());
    let header = message.header();
    let mut statement = connection.prepare_cached(&format!(
        "INSERT INTO messages (id, account_id, remote_kind, mailbox_id, uid_validity, uid, \
         gmail_message_id, gmail_thread_id, subject, sent_at, received_at, header_message_id, \
         in_reply_to, list_id, has_list_unsubscribe, size_bytes, flags) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17) \
         ON CONFLICT (id) DO UPDATE SET {UPDATED_COLUMNS}, {REMOTE_COLUMNS} \
         ON CONFLICT (mailbox_id, uid_validity, uid) WHERE remote_kind = 'imap' \
         DO UPDATE SET {UPDATED_COLUMNS} \
         ON CONFLICT (account_id, gmail_message_id) WHERE remote_kind = 'gmail' \
         DO UPDATE SET {UPDATED_COLUMNS}, uid_validity = excluded.uid_validity, \
         uid = excluded.uid, gmail_thread_id = excluded.gmail_thread_id \
         RETURNING id"
    ))?;
    let stored_id: String = statement.query_row(
        params![
            message.id().to_string(),
            message.account_id().to_string(),
            remote.kind,
            remote.mailbox_id,
            remote.uid_validity,
            remote.uid,
            remote.gmail_message_id,
            remote.gmail_thread_id,
            header.subject(),
            header.sent_at().map(UnixTime::seconds),
            header.received_at().seconds(),
            header.message_id().map(HeaderMessageId::as_str),
            header.in_reply_to().map(HeaderMessageId::as_str),
            header.list_id(),
            header.has_list_unsubscribe(),
            header.size_bytes(),
            flags_to_column(message.flags()),
        ],
        |row| row.get(0),
    )?;
    connection
        .prepare_cached("UPDATE message_labels SET received_at = ?2 WHERE message_id = ?1")?
        .execute(params![stored_id, header.received_at().seconds()])?;
    replace_children(connection, &stored_id, message)?;
    stored_id
        .parse()
        .map_err(|_| StoreError::CorruptRow { column: "id" })
}

/// Replaces the participants, references and keywords of the message stored as `id`.
fn replace_children(
    connection: &Connection,
    id: &str,
    message: &Message,
) -> Result<(), StoreError> {
    for table in [
        "message_participants",
        "message_references",
        "message_keywords",
    ] {
        connection
            .prepare_cached(&format!("DELETE FROM {table} WHERE message_id = ?1"))?
            .execute([id])?;
    }
    let mut participant = connection.prepare_cached(
        "INSERT INTO message_participants (message_id, field, position, name, address) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    for field in ParticipantField::ALL {
        // `zip(0_i64..)` numbers the entries without casting a `usize` index.
        for (entry, position) in field.of(message.header()).iter().zip(0_i64..) {
            participant.execute(params![
                id,
                field.as_column(),
                position,
                entry.name(),
                entry.address().as_str(),
            ])?;
        }
    }
    let mut reference = connection.prepare_cached(
        "INSERT INTO message_references (message_id, position, header_message_id) \
         VALUES (?1, ?2, ?3)",
    )?;
    for (entry, position) in message.header().references().iter().zip(0_i64..) {
        reference.execute(params![id, position, entry.as_str()])?;
    }
    let mut keyword = connection
        .prepare_cached("INSERT INTO message_keywords (message_id, keyword) VALUES (?1, ?2)")?;
    for entry in message.flags().keywords() {
        keyword.execute(params![id, entry.as_str()])?;
    }
    Ok(())
}

/// Loads the child rows of `row` and builds the message.
fn load_message(connection: &Connection, row: &MessageRow) -> Result<Message, StoreError> {
    let participants = connection
        .prepare_cached(
            "SELECT field, name, address FROM message_participants WHERE message_id = ?1 \
             ORDER BY field, position",
        )?
        .query_map([&row.id], |child| {
            Ok((child.get(0)?, child.get(1)?, child.get(2)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let references = connection
        .prepare_cached(
            "SELECT header_message_id FROM message_references WHERE message_id = ?1 \
             ORDER BY position",
        )?
        .query_map([&row.id], |child| child.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let keywords = connection
        .prepare_cached("SELECT keyword FROM message_keywords WHERE message_id = ?1")?
        .query_map([&row.id], |child| child.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    build_message(
        row,
        &MessageChildren {
            participants,
            references,
            keywords,
        },
    )
}

#[cfg(test)]
mod tests;
