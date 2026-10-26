//! Conversions between [`Message`] and the rows of `messages` and its child tables.

use clientmail_domain::{
    GmailMessageId, GmailThreadId, HeaderFields, ImapUid, Keyword, Message, MessageHeader,
    Participant, RemoteMessageRef, UidValidity, UnixTime,
};
use rusqlite::Row;

use crate::StoreError;
use crate::mapping::{flags_from_column, u32_from_column, u64_from_column, u64_to_column};

/// The columns of `messages`, in the order [`MessageRow::read`] expects, for the table alias `m`.
pub(crate) const MESSAGE_COLUMNS: &str = "m.id, m.account_id, m.remote_kind, m.mailbox_id, \
    m.uid_validity, m.uid, m.gmail_message_id, m.gmail_thread_id, m.subject, m.sent_at, \
    m.received_at, m.header_message_id, m.in_reply_to, m.list_id, m.has_list_unsubscribe, \
    m.size_bytes, m.flags";

/// The value of `message_participants.field` for each header that lists participants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParticipantField {
    From,
    To,
    Cc,
    Bcc,
    ReplyTo,
}

impl ParticipantField {
    pub(crate) const ALL: [Self; 5] = [Self::From, Self::To, Self::Cc, Self::Bcc, Self::ReplyTo];

    pub(crate) const fn as_column(self) -> &'static str {
        match self {
            Self::From => "from",
            Self::To => "to",
            Self::Cc => "cc",
            Self::Bcc => "bcc",
            Self::ReplyTo => "reply-to",
        }
    }

    /// Returns the participants of this field in `header`.
    pub(crate) fn of(self, header: &MessageHeader) -> &[Participant] {
        match self {
            Self::From => header.from(),
            Self::To => header.to(),
            Self::Cc => header.cc(),
            Self::Bcc => header.bcc(),
            Self::ReplyTo => header.reply_to(),
        }
    }

    fn from_column(text: &str) -> Result<Self, StoreError> {
        Self::ALL
            .into_iter()
            .find(|field| field.as_column() == text)
            .ok_or(StoreError::CorruptRow { column: "field" })
    }
}

/// How a [`RemoteMessageRef`] is spread over the columns of `messages`.
pub(crate) struct RemoteColumns {
    pub(crate) kind: &'static str,
    pub(crate) mailbox_id: Option<String>,
    pub(crate) uid_validity: u32,
    pub(crate) uid: u32,
    pub(crate) gmail_message_id: Option<i64>,
    pub(crate) gmail_thread_id: Option<i64>,
}

impl RemoteColumns {
    /// Spreads a server reference over its columns.
    pub(crate) fn of(remote: RemoteMessageRef) -> Self {
        match remote {
            RemoteMessageRef::Imap {
                mailbox,
                uid_validity,
                uid,
            } => Self {
                kind: "imap",
                mailbox_id: Some(mailbox.to_string()),
                uid_validity: uid_validity.get(),
                uid: uid.get(),
                gmail_message_id: None,
                gmail_thread_id: None,
            },
            RemoteMessageRef::Gmail {
                message_id,
                thread_id,
                all_mail_uid_validity,
                all_mail_uid,
            } => Self {
                kind: "gmail",
                mailbox_id: None,
                uid_validity: all_mail_uid_validity.get(),
                uid: all_mail_uid.get(),
                gmail_message_id: Some(u64_to_column(message_id.get())),
                gmail_thread_id: Some(u64_to_column(thread_id.get())),
            },
        }
    }
}

/// A `messages` row as `SQLite` returns it, before domain validation.
pub(crate) struct MessageRow {
    pub(crate) id: String,
    account_id: String,
    remote_kind: String,
    mailbox_id: Option<String>,
    uid_validity: i64,
    uid: i64,
    gmail_message_id: Option<i64>,
    gmail_thread_id: Option<i64>,
    subject: Option<String>,
    sent_at: Option<i64>,
    received_at: i64,
    header_message_id: Option<String>,
    in_reply_to: Option<String>,
    list_id: Option<String>,
    has_list_unsubscribe: bool,
    size_bytes: i64,
    flags: i64,
}

impl MessageRow {
    /// Returns the key the message list sorts by, descending: `received_at`, then id.
    pub(crate) fn list_key(&self) -> (i64, &str) {
        (self.received_at, &self.id)
    }

    /// Reads the columns listed in [`MESSAGE_COLUMNS`].
    pub(crate) fn read(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            account_id: row.get(1)?,
            remote_kind: row.get(2)?,
            mailbox_id: row.get(3)?,
            uid_validity: row.get(4)?,
            uid: row.get(5)?,
            gmail_message_id: row.get(6)?,
            gmail_thread_id: row.get(7)?,
            subject: row.get(8)?,
            sent_at: row.get(9)?,
            received_at: row.get(10)?,
            header_message_id: row.get(11)?,
            in_reply_to: row.get(12)?,
            list_id: row.get(13)?,
            has_list_unsubscribe: row.get(14)?,
            size_bytes: row.get(15)?,
            flags: row.get(16)?,
        })
    }

    /// Returns the header values held by this row, with empty participant lists.
    fn fields_without_participants(&self) -> Result<HeaderFields, StoreError> {
        Ok(HeaderFields {
            subject: self.subject.clone(),
            from: Vec::new(),
            to: Vec::new(),
            cc: Vec::new(),
            bcc: Vec::new(),
            reply_to: Vec::new(),
            sent_at: self.sent_at.map(UnixTime::from_seconds),
            received_at: UnixTime::from_seconds(self.received_at),
            message_id: self.header_message_id.as_deref().map(bracketed),
            in_reply_to: self.in_reply_to.as_deref().map(bracketed),
            references: Vec::new(),
            list_id: self.list_id.clone(),
            has_list_unsubscribe: self.has_list_unsubscribe,
            size_bytes: u32_from_column(self.size_bytes, "size_bytes")?,
        })
    }
}

/// The child rows of one message, in stored order.
pub(crate) struct MessageChildren {
    /// `(field, name, address)` ordered by field and position.
    pub(crate) participants: Vec<(String, Option<String>, String)>,
    pub(crate) references: Vec<String>,
    pub(crate) keywords: Vec<String>,
}

/// Rebuilds a message from its row and child rows.
///
/// Header values go back through [`MessageHeader::from_fields`]. Stored values are already
/// normalised, so this gives back the message that was stored: ids are wrapped in angle brackets
/// first, because a stored id has none and the parser expects them.
///
/// # Errors
///
/// Returns [`StoreError::CorruptRow`] when a value breaks a domain rule.
pub(crate) fn build_message(
    row: &MessageRow,
    children: &MessageChildren,
) -> Result<Message, StoreError> {
    let corrupt = |column| move |_| StoreError::CorruptRow { column };
    let id = row.id.parse().map_err(corrupt("id"))?;
    let account_id = row.account_id.parse().map_err(corrupt("account_id"))?;
    let remote = build_remote(row)?;
    let header = build_header(row.fields_without_participants()?, children)?;
    let mut flags = flags_from_column(row.flags)?;
    for keyword in &children.keywords {
        flags.insert_keyword(Keyword::parse(keyword).map_err(corrupt("keyword"))?);
    }
    Ok(Message::new(id, account_id, remote, header, flags))
}

fn build_header(
    mut fields: HeaderFields,
    children: &MessageChildren,
) -> Result<MessageHeader, StoreError> {
    for (field, name, address) in &children.participants {
        let participant = Participant::new(name.as_deref(), address)
            .ok_or(StoreError::CorruptRow { column: "address" })?;
        let list = match ParticipantField::from_column(field)? {
            ParticipantField::From => &mut fields.from,
            ParticipantField::To => &mut fields.to,
            ParticipantField::Cc => &mut fields.cc,
            ParticipantField::Bcc => &mut fields.bcc,
            ParticipantField::ReplyTo => &mut fields.reply_to,
        };
        list.push(participant);
    }
    fields.references = children
        .references
        .iter()
        .map(String::as_str)
        .map(bracketed)
        .collect();
    Ok(MessageHeader::from_fields(fields))
}

/// Wraps a stored message id in the angle brackets the header parser expects.
fn bracketed(id: &str) -> String {
    format!("<{id}>")
}

fn build_remote(row: &MessageRow) -> Result<RemoteMessageRef, StoreError> {
    let uid_validity = UidValidity::new(u32_from_column(row.uid_validity, "uid_validity")?).ok_or(
        StoreError::CorruptRow {
            column: "uid_validity",
        },
    )?;
    let uid = ImapUid::new(u32_from_column(row.uid, "uid")?)
        .ok_or(StoreError::CorruptRow { column: "uid" })?;
    match row.remote_kind.as_str() {
        "imap" => {
            let mailbox = row
                .mailbox_id
                .as_deref()
                .and_then(|id| id.parse().ok())
                .ok_or(StoreError::CorruptRow {
                    column: "mailbox_id",
                })?;
            Ok(RemoteMessageRef::Imap {
                mailbox,
                uid_validity,
                uid,
            })
        }
        "gmail" => {
            let message_id = row
                .gmail_message_id
                .map(|id| GmailMessageId::new(u64_from_column(id)))
                .ok_or(StoreError::CorruptRow {
                    column: "gmail_message_id",
                })?;
            let thread_id = row
                .gmail_thread_id
                .map(|id| GmailThreadId::new(u64_from_column(id)))
                .ok_or(StoreError::CorruptRow {
                    column: "gmail_thread_id",
                })?;
            Ok(RemoteMessageRef::Gmail {
                message_id,
                thread_id,
                all_mail_uid_validity: uid_validity,
                all_mail_uid: uid,
            })
        }
        _ => Err(StoreError::CorruptRow {
            column: "remote_kind",
        }),
    }
}
