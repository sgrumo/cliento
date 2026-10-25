use std::collections::HashSet;
use std::fmt;
use std::num::NonZeroU32;

use crate::id::uuid_id;
use crate::redact::{Redacted, redact_option};
use crate::{DomainError, MailboxId, Participant, UnixTime};

uuid_id!(
    /// Identifies a message on this Mac. Never changes, even when the server renumbers messages.
    MessageId,
    DomainError::InvalidMessageId
);

/// An IMAP message `UID`. Never zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ImapUid(NonZeroU32);

impl ImapUid {
    /// Wraps a `UID` from the server.
    ///
    /// # Returns
    ///
    /// `None` when `uid` is 0, which IMAP never assigns.
    #[must_use]
    pub const fn new(uid: u32) -> Option<Self> {
        match NonZeroU32::new(uid) {
            Some(uid) => Some(Self(uid)),
            None => None,
        }
    }

    /// Returns the raw value.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

/// An IMAP mailbox `UIDVALIDITY`. When it changes, every `UID` in the mailbox is invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UidValidity(NonZeroU32);

impl UidValidity {
    /// Wraps a `UIDVALIDITY` from the server.
    ///
    /// # Returns
    ///
    /// `None` when `value` is 0, which IMAP never assigns.
    #[must_use]
    pub const fn new(value: u32) -> Option<Self> {
        match NonZeroU32::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// Returns the raw value.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

/// Gmail's id for a message (`X-GM-MSGID`): the same in every label the message carries.
///
/// The full `u64` range is valid. `SQLite` integers are signed, so storage must keep the bits of
/// the value, not its numeric value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GmailMessageId(u64);

impl GmailMessageId {
    /// Wraps the id from the server.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the raw value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Gmail's id for a conversation (`X-GM-THRID`), shared by every message in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GmailThreadId(u64);

impl GmailThreadId {
    /// Wraps the id from the server.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the raw value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Where a message lives on its server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RemoteMessageRef {
    /// A message on a plain IMAP server, identified inside one mailbox.
    Imap {
        /// The mailbox that holds the message. For plain IMAP this is the source of truth for
        /// which mailbox the message is in.
        mailbox: MailboxId,
        /// The mailbox's `UIDVALIDITY` when the `UID` was read.
        uid_validity: UidValidity,
        /// The message `UID` inside the mailbox.
        uid: ImapUid,
    },
    /// A Gmail message. Labels are stored separately; actions run against `[Gmail]/All Mail`.
    Gmail {
        /// The message id, stable across labels.
        message_id: GmailMessageId,
        /// The conversation the message belongs to.
        thread_id: GmailThreadId,
        /// The `UIDVALIDITY` of `[Gmail]/All Mail` when `all_mail_uid` was read.
        all_mail_uid_validity: UidValidity,
        /// The message `UID` in `[Gmail]/All Mail`, used for every write and for expunge
        /// detection.
        all_mail_uid: ImapUid,
    },
}

/// An RFC 5322 message id (`Message-ID`, `In-Reply-To`, `References`), without angle brackets.
///
/// Not to be confused with [`MessageId`], the local id. `Debug` hides the value, because ids often
/// contain the sender's domain or address.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct HeaderMessageId(String);

impl HeaderMessageId {
    /// Extracts the message id from a header value.
    ///
    /// Takes the last `<...>` in the text, so comments and phrases around it are ignored. Without
    /// brackets, takes the last whitespace-separated word that contains `@`.
    ///
    /// # Arguments
    ///
    /// * `raw` - The header value, for example `Your message of Monday <abc@example.com>`.
    ///
    /// # Returns
    ///
    /// The id, or `None` when the text holds none.
    ///
    /// # Examples
    ///
    /// ```
    /// use clientmail_domain::HeaderMessageId;
    ///
    /// let id = HeaderMessageId::parse("<abc@example.com> (sent from my phone)").unwrap();
    /// assert_eq!(id.as_str(), "abc@example.com");
    /// ```
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        message_id_tokens(raw)
            .pop()
            .map(|token| Self(token.to_owned()))
    }

    /// Returns the id text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for HeaderMessageId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("HeaderMessageId")
            .field(&Redacted)
            .finish()
    }
}

/// Returns every message id in a header value: the `<...>` tokens when there are any, otherwise
/// the whitespace-separated words that contain `@`.
///
/// The returned slices borrow from `raw`, so nothing is copied until a caller keeps an id.
fn message_id_tokens(raw: &str) -> Vec<&str> {
    let bracketed: Vec<&str> = raw
        .split('<')
        .skip(1)
        .filter_map(|part| part.split_once('>'))
        .map(|(inside, _)| inside.trim())
        .filter(|inside| !inside.is_empty())
        .collect();
    if !bracketed.is_empty() {
        return bracketed;
    }
    raw.split_whitespace()
        .filter(|word| word.contains('@'))
        .map(|word| word.trim_matches(['<', '>']))
        .filter(|word| !word.is_empty())
        .collect()
}

/// Raw header values, as decoded by a protocol adapter. Input to [`MessageHeader::from_fields`].
///
/// Fields are public because this is a plain parameter object with no rules of its own. `Debug`
/// hides personal data.
#[derive(Clone, PartialEq, Eq)]
pub struct HeaderFields {
    /// The `Subject` header.
    pub subject: Option<String>,
    /// The `From` header.
    pub from: Vec<Participant>,
    /// The `To` header.
    pub to: Vec<Participant>,
    /// The `Cc` header.
    pub cc: Vec<Participant>,
    /// The `Bcc` header. Present only on messages the user sent.
    pub bcc: Vec<Participant>,
    /// The `Reply-To` header.
    pub reply_to: Vec<Participant>,
    /// The `Date` header: when the sender says the message was sent.
    pub sent_at: Option<UnixTime>,
    /// When the server received the message (IMAP `INTERNALDATE`).
    pub received_at: UnixTime,
    /// The `Message-ID` header value.
    pub message_id: Option<String>,
    /// The `In-Reply-To` header value.
    pub in_reply_to: Option<String>,
    /// The `References` header values. Each element may hold one id or a whole header line.
    pub references: Vec<String>,
    /// The `List-Id` header value.
    pub list_id: Option<String>,
    /// True when the message has a `List-Unsubscribe` header.
    pub has_list_unsubscribe: bool,
    /// The size of the whole message, in bytes.
    pub size_bytes: u32,
}

impl fmt::Debug for HeaderFields {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HeaderFields")
            .field("subject", &redact_option(self.subject.as_ref()))
            .field("from", &self.from)
            .field("to", &self.to)
            .field("cc", &self.cc)
            .field("bcc", &self.bcc)
            .field("reply_to", &self.reply_to)
            .field("sent_at", &self.sent_at)
            .field("received_at", &self.received_at)
            .field("message_id", &redact_option(self.message_id.as_ref()))
            .field("in_reply_to", &redact_option(self.in_reply_to.as_ref()))
            .field("references", &self.references.len())
            .field("list_id", &redact_option(self.list_id.as_ref()))
            .field("has_list_unsubscribe", &self.has_list_unsubscribe)
            .field("size_bytes", &self.size_bytes)
            .finish()
    }
}

/// The header of a message: what the message list needs, plus the ids threading and Smart Inbox
/// need.
///
/// The message list sorts by [`MessageHeader::received_at`], like Gmail and Apple Mail: a message
/// that arrives late still appears at the top. `Debug` hides personal data.
#[derive(Clone, PartialEq, Eq)]
pub struct MessageHeader {
    subject: Option<String>,
    from: Vec<Participant>,
    to: Vec<Participant>,
    cc: Vec<Participant>,
    bcc: Vec<Participant>,
    reply_to: Vec<Participant>,
    sent_at: Option<UnixTime>,
    received_at: UnixTime,
    message_id: Option<HeaderMessageId>,
    in_reply_to: Option<HeaderMessageId>,
    references: Vec<HeaderMessageId>,
    list_id: Option<String>,
    has_list_unsubscribe: bool,
    size_bytes: u32,
}

impl MessageHeader {
    /// Normalises raw header values. Never fails: a client must show whatever the server sent.
    ///
    /// The subject and list id are trimmed and dropped when empty. Message ids are extracted with
    /// [`HeaderMessageId::parse`]; `References` values are split into all their ids, and repeated
    /// ids keep only their first occurrence.
    ///
    /// # Arguments
    ///
    /// * `fields` - The decoded header values. Participant lists move in without copying; the
    ///   subject is reused when it needs no trimming.
    #[must_use]
    pub fn from_fields(fields: HeaderFields) -> Self {
        let mut seen_references = HashSet::new();
        let references = fields
            .references
            .iter()
            .flat_map(|raw| message_id_tokens(raw))
            // `insert` returns false for a value already in the set, which drops repeats. The
            // set holds borrowed slices, so only kept ids are copied.
            .filter(|token| seen_references.insert(*token))
            .map(|token| HeaderMessageId(token.to_owned()))
            .collect();
        Self {
            subject: fields.subject.and_then(trimmed),
            from: fields.from,
            to: fields.to,
            cc: fields.cc,
            bcc: fields.bcc,
            reply_to: fields.reply_to,
            sent_at: fields.sent_at,
            received_at: fields.received_at,
            message_id: fields
                .message_id
                .as_deref()
                .and_then(HeaderMessageId::parse),
            in_reply_to: fields
                .in_reply_to
                .as_deref()
                .and_then(HeaderMessageId::parse),
            references,
            list_id: fields.list_id.and_then(trimmed),
            has_list_unsubscribe: fields.has_list_unsubscribe,
            size_bytes: fields.size_bytes,
        }
    }

    /// Returns the subject, if the message has a non-empty one.
    #[must_use]
    pub fn subject(&self) -> Option<&str> {
        self.subject.as_deref()
    }

    /// Returns the senders.
    #[must_use]
    pub fn from(&self) -> &[Participant] {
        &self.from
    }

    /// Returns the direct recipients.
    #[must_use]
    pub fn to(&self) -> &[Participant] {
        &self.to
    }

    /// Returns the copied recipients.
    #[must_use]
    pub fn cc(&self) -> &[Participant] {
        &self.cc
    }

    /// Returns the blind-copied recipients. Empty except on messages the user sent.
    #[must_use]
    pub fn bcc(&self) -> &[Participant] {
        &self.bcc
    }

    /// Returns where replies should go, when it differs from the senders.
    #[must_use]
    pub fn reply_to(&self) -> &[Participant] {
        &self.reply_to
    }

    /// Returns when the sender says the message was sent. Shown in the reader, not used to sort.
    #[must_use]
    pub const fn sent_at(&self) -> Option<UnixTime> {
        self.sent_at
    }

    /// Returns when the server received the message. The message list sorts by this time.
    #[must_use]
    pub const fn received_at(&self) -> UnixTime {
        self.received_at
    }

    /// Returns the `Message-ID`.
    #[must_use]
    pub const fn message_id(&self) -> Option<&HeaderMessageId> {
        self.message_id.as_ref()
    }

    /// Returns the `In-Reply-To` id.
    #[must_use]
    pub const fn in_reply_to(&self) -> Option<&HeaderMessageId> {
        self.in_reply_to.as_ref()
    }

    /// Returns the referenced message ids, oldest first, without repeats.
    #[must_use]
    pub fn references(&self) -> &[HeaderMessageId] {
        &self.references
    }

    /// Returns the `List-Id` value, when the message came from a mailing list.
    #[must_use]
    pub fn list_id(&self) -> Option<&str> {
        self.list_id.as_deref()
    }

    /// Returns true when the message offers a `List-Unsubscribe` link.
    #[must_use]
    pub const fn has_list_unsubscribe(&self) -> bool {
        self.has_list_unsubscribe
    }

    /// Returns the message size in bytes.
    #[must_use]
    pub const fn size_bytes(&self) -> u32 {
        self.size_bytes
    }
}

impl fmt::Debug for MessageHeader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MessageHeader")
            .field("subject", &redact_option(self.subject.as_ref()))
            .field("from", &self.from)
            .field("to", &self.to)
            .field("cc", &self.cc)
            .field("bcc", &self.bcc)
            .field("reply_to", &self.reply_to)
            .field("sent_at", &self.sent_at)
            .field("received_at", &self.received_at)
            .field("message_id", &self.message_id)
            .field("in_reply_to", &self.in_reply_to)
            .field("references", &self.references.len())
            .field("list_id", &redact_option(self.list_id.as_ref()))
            .field("has_list_unsubscribe", &self.has_list_unsubscribe)
            .field("size_bytes", &self.size_bytes)
            .finish()
    }
}

/// Trims `text`, reusing its allocation when there is nothing to trim. Returns `None` when
/// nothing is left.
fn trimmed(text: String) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else if trimmed.len() == text.len() {
        Some(text)
    } else {
        Some(trimmed.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields() -> HeaderFields {
        HeaderFields {
            subject: Some("Hello".to_owned()),
            from: Participant::new(Some("Ada"), "ada@example.com")
                .into_iter()
                .collect(),
            to: Vec::new(),
            cc: Vec::new(),
            bcc: Vec::new(),
            reply_to: Vec::new(),
            sent_at: Some(UnixTime::from_seconds(1_000)),
            received_at: UnixTime::from_seconds(2_000),
            message_id: Some("<abc@example.com>".to_owned()),
            in_reply_to: None,
            references: Vec::new(),
            list_id: None,
            has_list_unsubscribe: false,
            size_bytes: 512,
        }
    }

    fn ids(header: &MessageHeader) -> Vec<&str> {
        header
            .references()
            .iter()
            .map(HeaderMessageId::as_str)
            .collect()
    }

    #[test]
    fn subject_and_list_id_are_trimmed_and_empty_becomes_none() {
        let trimmed = MessageHeader::from_fields(HeaderFields {
            subject: Some("  Hello  ".to_owned()),
            list_id: Some(" News <news.example.com> ".to_owned()),
            ..fields()
        });
        let blank = MessageHeader::from_fields(HeaderFields {
            subject: Some("   ".to_owned()),
            list_id: Some(" ".to_owned()),
            ..fields()
        });

        assert_eq!(trimmed.subject(), Some("Hello"), "subject trimmed");
        assert_eq!(
            trimmed.list_id(),
            Some("News <news.example.com>"),
            "list id trimmed"
        );
        assert_eq!(blank.subject(), None, "blank subject dropped");
        assert_eq!(blank.list_id(), None, "blank list id dropped");
    }

    #[test]
    fn bcc_and_list_unsubscribe_are_kept() {
        let header = MessageHeader::from_fields(HeaderFields {
            bcc: Participant::new(None, "bob@example.com")
                .into_iter()
                .collect(),
            has_list_unsubscribe: true,
            ..fields()
        });

        assert_eq!(header.bcc().len(), 1, "bcc kept");
        assert!(header.has_list_unsubscribe(), "list unsubscribe kept");
    }

    #[test]
    fn header_message_id_ignores_comments_and_phrases() {
        let cases = [
            ("  <abc@example.com> ", Some("abc@example.com")),
            (
                "<abc@example.com> (sent from my phone)",
                Some("abc@example.com"),
            ),
            (
                "Your message of Mon, 1 Jan <id@example.com>",
                Some("id@example.com"),
            ),
            ("abc@example.com", Some("abc@example.com")),
            ("<>", None),
            ("Your message of Monday", None),
        ];

        for (raw, expected) in cases {
            assert_eq!(
                HeaderMessageId::parse(raw)
                    .as_ref()
                    .map(HeaderMessageId::as_str),
                expected,
                "{raw:?}"
            );
        }
    }

    #[test]
    fn message_id_fields_use_the_parser() {
        let header = MessageHeader::from_fields(HeaderFields {
            message_id: Some("<abc@example.com> (comment)".to_owned()),
            in_reply_to: Some("<>".to_owned()),
            ..fields()
        });

        assert_eq!(
            header.message_id().map(HeaderMessageId::as_str),
            Some("abc@example.com"),
            "comment ignored"
        );
        assert_eq!(header.in_reply_to(), None, "empty id dropped");
    }

    #[test]
    fn references_split_whole_header_lines() {
        let header = MessageHeader::from_fields(HeaderFields {
            references: vec!["<a@x> <b@x>\r\n <c@x>".to_owned()],
            ..fields()
        });

        assert_eq!(ids(&header), ["a@x", "b@x", "c@x"], "one id per token");
    }

    #[test]
    fn references_are_deduplicated_in_order() {
        let header = MessageHeader::from_fields(HeaderFields {
            references: vec![
                "<one@x>".to_owned(),
                "<two@x> <one@x>".to_owned(),
                " ".to_owned(),
                "three@x".to_owned(),
            ],
            ..fields()
        });

        assert_eq!(
            ids(&header),
            ["one@x", "two@x", "three@x"],
            "first occurrence kept, blanks dropped"
        );
    }

    #[test]
    fn received_at_is_kept_for_sorting_even_when_sent_at_is_old() {
        let header = MessageHeader::from_fields(HeaderFields {
            sent_at: Some(UnixTime::from_seconds(0)),
            received_at: UnixTime::from_seconds(1_700_000_000),
            ..fields()
        });

        assert_eq!(
            header.received_at(),
            UnixTime::from_seconds(1_700_000_000),
            "sort key"
        );
        assert_eq!(
            header.sent_at(),
            Some(UnixTime::from_seconds(0)),
            "sender date kept for display"
        );
    }

    #[test]
    fn gmail_reference_carries_ids_and_all_mail_uid() {
        let reference = RemoteMessageRef::Gmail {
            message_id: GmailMessageId::new(u64::MAX),
            thread_id: GmailThreadId::new(42),
            all_mail_uid_validity: UidValidity::new(1).unwrap(),
            all_mail_uid: ImapUid::new(7).unwrap(),
        };

        let RemoteMessageRef::Gmail {
            message_id,
            thread_id,
            all_mail_uid,
            ..
        } = reference
        else {
            panic!("expected a Gmail reference");
        };

        assert_eq!(message_id.get(), u64::MAX, "full u64 range");
        assert_eq!(thread_id.get(), 42, "thread id");
        assert_eq!(all_mail_uid.get(), 7, "All Mail uid");
    }

    #[test]
    fn uid_and_uid_validity_reject_zero() {
        assert_eq!(ImapUid::new(0), None, "uid 0");
        assert_eq!(UidValidity::new(0), None, "uidvalidity 0");
        assert_eq!(ImapUid::new(7).map(ImapUid::get), Some(7), "uid round trip");
        assert_eq!(
            UidValidity::new(9).map(UidValidity::get),
            Some(9),
            "uidvalidity round trip"
        );
    }
}
