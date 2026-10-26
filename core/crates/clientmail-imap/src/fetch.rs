//! Building header fetches and reading their results, without I/O.

use std::collections::{BTreeMap, HashSet};
use std::num::NonZeroU32;

use clientmail_domain::{
    Flag, GmailItems, GmailLabel, GmailMessageId, GmailThreadId, HeaderBatch, ImapUid, Keyword,
    MessageFlags, MessageHeader, RemoteHeader, RemoteName, SystemLabel, UnixTime,
};
use imap_next::imap_types::core::{AString, NString, Vec1};
use imap_next::imap_types::fetch::{GmLabel, MessageDataItem, MessageDataItemName, Section};
use imap_next::imap_types::flag::{Flag as ImapFlag, FlagFetch};

/// Returns the day to search from: one day before `since`'s UTC day.
///
/// `SEARCH SINCE` compares whole days of each message's `INTERNALDATE` in the server's own zone
/// (RFC 3501, 6.4.4). Starting a day early means no zone west of UTC can drop a message that
/// arrived after `since`; a few earlier messages come back too.
pub(crate) fn search_day(since: UnixTime) -> Option<chrono::NaiveDate> {
    chrono::DateTime::from_timestamp(since.seconds(), 0)?
        .date_naive()
        .pred_opt()
}

/// The `FETCH` responses of one command, merged per sequence number.
#[derive(Default)]
pub(crate) struct Responses {
    items: BTreeMap<NonZeroU32, Vec<MessageDataItem<'static>>>,
}

impl Responses {
    /// Adds one response's items to its message's.
    pub(crate) fn add(&mut self, seq: NonZeroU32, items: Vec<MessageDataItem<'static>>) {
        self.items.entry(seq).or_default().extend(items);
    }

    /// Turns the merged responses into headers for the `UID`s that were asked for, each once.
    ///
    /// `skipped` counts the `UID`s asked for that did not come back complete: a response that could
    /// not be parsed, items missing, or no response at all. Responses for other messages (flag
    /// changes the server pushes) neither add a header nor count.
    pub(crate) fn into_batch(self, asked: &[ImapUid], gmail: bool) -> HeaderBatch {
        let wanted: HashSet<ImapUid> = asked.iter().copied().collect();
        let mut seen = HashSet::new();
        let headers: Vec<RemoteHeader> = self
            .items
            .into_values()
            .filter_map(|items| remote_header(items, gmail))
            .filter(|header| wanted.contains(&header.uid()) && seen.insert(header.uid()))
            .collect();
        let skipped = wanted.len() - headers.len();
        HeaderBatch::new(headers, skipped)
    }
}

/// The header fields fetched: enough for the message list, threading, Smart Inbox's list
/// detection, and `Bcc` on sent mail (note 1.1 P1, P2).
const HEADER_FIELDS: [&str; 12] = [
    "DATE",
    "FROM",
    "TO",
    "CC",
    "BCC",
    "REPLY-TO",
    "SUBJECT",
    "MESSAGE-ID",
    "IN-REPLY-TO",
    "REFERENCES",
    "LIST-ID",
    "LIST-UNSUBSCRIBE",
];

/// Writes `uids` as an IMAP sequence set, sorted, without repeats, with runs as ranges
/// (`3,7,8,9` becomes `3,7:9`).
pub(crate) fn sequence_set(uids: &[ImapUid]) -> String {
    let mut sorted: Vec<u32> = uids.iter().map(|uid| uid.get()).collect();
    sorted.sort_unstable();
    sorted.dedup();
    let mut ranges: Vec<(u32, u32)> = Vec::new();
    for uid in sorted {
        match ranges.last_mut() {
            Some((_, end)) if end.checked_add(1) == Some(uid) => *end = uid,
            Some(_) | None => ranges.push((uid, uid)),
        }
    }
    ranges
        .iter()
        .map(|&(start, end)| {
            if start == end {
                start.to_string()
            } else {
                format!("{start}:{end}")
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// The items a header fetch asks for; Gmail's ids and labels when the server has them.
pub(crate) fn item_names(gmail: bool) -> Vec<MessageDataItemName<'static>> {
    let fields = HEADER_FIELDS
        .iter()
        .filter_map(|name| AString::try_from(*name).ok())
        .collect::<Vec<_>>();
    let mut names = vec![
        MessageDataItemName::Uid,
        MessageDataItemName::Flags,
        MessageDataItemName::InternalDate,
        MessageDataItemName::Rfc822Size,
    ];
    if let Ok(fields) = Vec1::try_from(fields) {
        names.push(MessageDataItemName::BodyExt {
            section: Some(Section::HeaderFields(None, fields)),
            partial: None,
            // `PEEK` leaves `\Seen` alone: fetching must never mark mail as read.
            peek: true,
        });
    }
    if gmail {
        names.extend([
            MessageDataItemName::XGmMsgId,
            MessageDataItemName::XGmThrId,
            MessageDataItemName::XGmLabels,
        ]);
    }
    names
}

/// Reads one message's `FETCH` items into a header.
///
/// # Returns
///
/// `None` when an item the message cannot do without is missing: its `UID`, its arrival time, its
/// header section, or, on Gmail, its message and thread ids. The caller counts it as skipped.
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "items not asked for (BODYSTRUCTURE, ENVELOPE, ...) are ignored"
)]
pub(crate) fn remote_header(items: Vec<MessageDataItem<'_>>, gmail: bool) -> Option<RemoteHeader> {
    let mut uid = None;
    let mut flags = MessageFlags::default();
    let mut received_at = None;
    let mut size = 0;
    let mut raw_header = None;
    let mut message_id = None;
    let mut thread_id = None;
    let mut labels = Vec::new();
    for item in items {
        match item {
            MessageDataItem::Uid(value) => uid = ImapUid::new(value.get()),
            MessageDataItem::Flags(values) => flags = message_flags(&values),
            MessageDataItem::InternalDate(date) => {
                received_at = Some(UnixTime::from_seconds(date.as_ref().timestamp()));
            }
            MessageDataItem::Rfc822Size(value) => size = value,
            MessageDataItem::BodyExt { data, .. } => raw_header = Some(data),
            MessageDataItem::XGmMsgId(value) => message_id = Some(GmailMessageId::new(value)),
            MessageDataItem::XGmThrId(value) => thread_id = Some(GmailThreadId::new(value)),
            MessageDataItem::XGmLabels(values) => labels = values.iter().map(gmail_label).collect(),
            _ => {}
        }
    }
    let gmail_items = if gmail {
        Some(GmailItems::new(message_id?, thread_id?, labels))
    } else {
        None
    };
    let fields =
        clientmail_mime::parse_header_fields(nstring_bytes(&raw_header?), received_at?, size);
    Some(RemoteHeader::new(
        uid?,
        flags,
        MessageHeader::from_fields(fields),
        gmail_items,
    ))
}

/// Maps the server's flags (RFC 3501 system flags and keywords) to the domain's flags. `\Recent`
/// and unknown `\`-flags have no meaning here and are dropped; `$Forwarded` is a flag of its own.
fn message_flags(values: &[FlagFetch<'_>]) -> MessageFlags {
    let flags = values.iter().filter_map(|value| match value {
        FlagFetch::Flag(flag) => Some(flag),
        FlagFetch::Recent => None,
    });
    let mut collected: MessageFlags = flags.clone().filter_map(system_flag).collect();
    collected.extend(flags.filter_map(|flag| match flag {
        ImapFlag::Keyword(atom) if !atom.as_ref().eq_ignore_ascii_case("$Forwarded") => {
            Keyword::parse(atom.as_ref()).ok()
        }
        ImapFlag::Answered
        | ImapFlag::Deleted
        | ImapFlag::Draft
        | ImapFlag::Flagged
        | ImapFlag::Seen
        | ImapFlag::Keyword(_)
        | ImapFlag::Extension(_) => None,
    }));
    collected
}

fn system_flag(flag: &ImapFlag<'_>) -> Option<Flag> {
    match flag {
        ImapFlag::Seen => Some(Flag::Seen),
        ImapFlag::Flagged => Some(Flag::Flagged),
        ImapFlag::Answered => Some(Flag::Answered),
        ImapFlag::Draft => Some(Flag::Draft),
        ImapFlag::Deleted => Some(Flag::Deleted),
        ImapFlag::Keyword(atom) if atom.as_ref().eq_ignore_ascii_case("$Forwarded") => {
            Some(Flag::Forwarded)
        }
        ImapFlag::Keyword(_) | ImapFlag::Extension(_) => None,
    }
}

/// Maps one label. Gmail sends system labels as `\Inbox`, but also quoted (`"\\Important"`), so
/// a name that starts with a backslash is a system label too; a user label cannot start with one.
fn gmail_label(label: &GmLabel<'_>) -> GmailLabel {
    match label {
        GmLabel::System(atom) => GmailLabel::System(system_label(atom.as_ref())),
        GmLabel::Name(name) => {
            let bytes: &[u8] = name.as_ref();
            match bytes.strip_prefix(b"\\").map(std::str::from_utf8) {
                Some(Ok(system)) => GmailLabel::System(system_label(system)),
                Some(Err(_)) | None => GmailLabel::Name(RemoteName::new(bytes.to_vec())),
            }
        }
    }
}

/// Gmail's system labels (<https://developers.google.com/gmail/imap/imap-extensions>). Google's
/// examples spell drafts both `\Draft` and `\Drafts`.
fn system_label(name: &str) -> SystemLabel {
    let known = [
        ("Inbox", SystemLabel::Inbox),
        ("Sent", SystemLabel::Sent),
        ("Draft", SystemLabel::Draft),
        ("Drafts", SystemLabel::Draft),
        ("Important", SystemLabel::Important),
        ("Starred", SystemLabel::Starred),
        ("Trash", SystemLabel::Trash),
        ("Spam", SystemLabel::Spam),
    ];
    known
        .into_iter()
        .find(|(label, _)| name.eq_ignore_ascii_case(label))
        .map_or_else(|| SystemLabel::Other(name.to_owned()), |(_, label)| label)
}

fn nstring_bytes<'a>(value: &'a NString<'_>) -> &'a [u8] {
    value.0.as_ref().map(AsRef::as_ref).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use imap_next::imap_types::core::{Atom, IString, Literal, Quoted};
    use imap_next::imap_types::datetime::DateTime;

    use super::*;

    fn uids(values: &[u32]) -> Vec<ImapUid> {
        values
            .iter()
            .filter_map(|value| ImapUid::new(*value))
            .collect()
    }

    #[test]
    fn searches_start_the_day_before_the_utc_day() {
        let day = |seconds| search_day(UnixTime::from_seconds(seconds)).map(|day| day.to_string());
        // 2026-10-26 00:00:00 UTC, and one second before it.
        assert_eq!(day(1_792_972_800), Some("2026-10-25".into()), "midnight");
        assert_eq!(
            day(1_792_972_799),
            Some("2026-10-24".into()),
            "a second earlier"
        );
        assert_eq!(day(i64::MAX), None, "out of range");
    }

    #[test]
    fn sequence_sets_are_sorted_deduplicated_and_compressed() {
        assert_eq!(
            sequence_set(&uids(&[9, 3, 7, 8, 8, 1, 2])),
            "1:3,7:9",
            "ranges"
        );
        assert_eq!(sequence_set(&uids(&[5])), "5", "single");
        assert_eq!(
            sequence_set(&uids(&[4_294_967_295, 1])),
            "1,4294967295",
            "no overflow"
        );
    }

    #[test]
    fn gmail_items_are_asked_for_only_on_gmail() {
        let plain = item_names(false);
        let gmail = item_names(true);

        assert!(
            !plain.contains(&MessageDataItemName::XGmMsgId),
            "plain IMAP"
        );
        assert!(gmail.contains(&MessageDataItemName::XGmLabels), "Gmail");
        assert!(
            plain
                .iter()
                .any(|name| matches!(name, MessageDataItemName::BodyExt { peek: true, .. })),
            "headers fetched with PEEK"
        );
    }

    fn items(gmail: bool) -> Vec<MessageDataItem<'static>> {
        let header = b"From: Ada <ada@example.com>\r\nSubject: Hi\r\n\r\n".to_vec();
        let mut items = vec![
            MessageDataItem::Uid(NonZeroU32::new(42).unwrap()),
            MessageDataItem::Flags(vec![
                FlagFetch::Flag(ImapFlag::Seen),
                FlagFetch::Recent,
                FlagFetch::Flag(ImapFlag::Keyword(Atom::try_from("$Forwarded").unwrap())),
                FlagFetch::Flag(ImapFlag::Keyword(Atom::try_from("Work").unwrap())),
            ]),
            MessageDataItem::InternalDate(DateTime::unvalidated(
                chrono::DateTime::from_timestamp(1_700_000_000, 0)
                    .unwrap()
                    .fixed_offset(),
            )),
            MessageDataItem::Rfc822Size(1234),
            MessageDataItem::BodyExt {
                section: None,
                origin: None,
                data: NString(Some(IString::Literal(Literal::try_from(header).unwrap()))),
            },
        ];
        if gmail {
            items.extend([
                MessageDataItem::XGmMsgId(1_278_455_344_230_334_865),
                MessageDataItem::XGmThrId(1_278_455_344_230_334_000),
                MessageDataItem::XGmLabels(vec![
                    GmLabel::System(Atom::try_from("Inbox").unwrap()),
                    GmLabel::System(Atom::try_from("Unknown").unwrap()),
                    GmLabel::Name(AString::String(IString::Quoted(
                        Quoted::try_from("\\Important").unwrap(),
                    ))),
                    GmLabel::System(Atom::try_from("Drafts").unwrap()),
                    GmLabel::Name(AString::try_from("Clients/Acme").unwrap()),
                ]),
            ]);
        }
        items
    }

    #[test]
    fn a_plain_message_is_read_with_its_flags() {
        let header = remote_header(items(false), false).unwrap();

        assert_eq!(header.uid().get(), 42, "uid");
        assert!(header.flags().contains(Flag::Seen), "\\Seen");
        assert!(
            header.flags().contains(Flag::Forwarded),
            "$Forwarded is a flag"
        );
        assert_eq!(
            header
                .flags()
                .keywords()
                .map(Keyword::as_str)
                .collect::<Vec<_>>(),
            ["Work"],
            "keywords kept, \\Recent dropped"
        );
        assert_eq!(
            header.header().received_at().seconds(),
            1_700_000_000,
            "INTERNALDATE"
        );
        assert_eq!(header.header().size_bytes(), 1234, "size");
        assert_eq!(header.header().subject(), Some("Hi"), "header parsed");
        assert!(header.gmail().is_none(), "no Gmail items");
    }

    #[test]
    fn a_gmail_message_carries_its_ids_and_labels() {
        let header = remote_header(items(true), true).unwrap();
        let gmail = header.gmail().unwrap();

        assert_eq!(
            gmail.message_id().get(),
            1_278_455_344_230_334_865,
            "message id"
        );
        assert_eq!(
            gmail.thread_id().get(),
            1_278_455_344_230_334_000,
            "thread id"
        );
        assert_eq!(
            gmail.labels(),
            [
                GmailLabel::System(SystemLabel::Inbox),
                GmailLabel::System(SystemLabel::Other("Unknown".to_owned())),
                GmailLabel::System(SystemLabel::Important),
                GmailLabel::System(SystemLabel::Draft),
                GmailLabel::Name(RemoteName::new(b"Clients/Acme".to_vec())),
            ],
            "labels"
        );
    }

    #[test]
    fn a_message_missing_what_it_needs_is_skipped() {
        let mut no_uid = items(false);
        no_uid.remove(0);
        let mut no_date = items(false);
        no_date.remove(2);

        assert!(remote_header(no_uid, false).is_none(), "no UID");
        assert!(remote_header(no_date, false).is_none(), "no INTERNALDATE");
        assert!(
            remote_header(items(false), true).is_none(),
            "Gmail ids asked for but missing"
        );
    }
}
