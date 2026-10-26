//! RFC 5322 message parsing for the `ClientMail` core.
//!
//! Turns raw header bytes into the domain's [`HeaderFields`], with `mail-parser`: RFC 2047
//! encoded words, address lists and groups, dates. Body parsing (step 2.1) will live here too.
//! It holds no rules: normalising the values is [`clientmail_domain::MessageHeader`]'s job.

use clientmail_domain::{HeaderFields, Participant, UnixTime};
use mail_parser::{AddressList, MessageParser};

/// Parses a message's header fields.
///
/// Never fails: mail is often malformed, and a client must show what the server sent. Missing or
/// unreadable fields are left empty, a date without a zone included; an address `mail-parser`
/// cannot read as an address is kept as text by [`Participant`]; group syntax is flattened into
/// its members. Message ids keep their angle brackets, as [`clientmail_domain::MessageHeader`]
/// expects.
///
/// # Arguments
///
/// * `raw` - The header section as fetched (`BODY[HEADER.FIELDS (...)]` or a whole message).
/// * `received_at` - When the server received the message (IMAP `INTERNALDATE`).
/// * `size_bytes` - The message's full size (IMAP `RFC822.SIZE`).
///
/// # Examples
///
/// ```
/// use clientmail_domain::UnixTime;
///
/// let raw = b"From: Ada <ada@example.com>\r\nSubject: =?utf-8?q?Caf=C3=A9?=\r\n\r\n";
/// let fields = clientmail_mime::parse_header_fields(raw, UnixTime::from_seconds(0), 120);
/// assert_eq!(fields.subject.as_deref(), Some("Café"));
/// assert_eq!(fields.from[0].name(), Some("Ada"));
/// ```
#[must_use]
pub fn parse_header_fields(raw: &[u8], received_at: UnixTime, size_bytes: u32) -> HeaderFields {
    let Some(message) = MessageParser::new().parse_headers(raw) else {
        return empty(received_at, size_bytes);
    };
    // `mail-parser` strips the angle brackets; putting them back lets the domain take each id
    // whole, even one without an `@`.
    let bracketed = |id: &str| format!("<{id}>");
    let list_id = message.list_id().and_then(|list| {
        list.mailboxes().find_map(|mailbox| {
            mailbox
                .address()
                .or_else(|| mailbox.name())
                .map(ToOwned::to_owned)
        })
    });
    HeaderFields {
        subject: message.subject().map(ToOwned::to_owned),
        from: participants(message.from()),
        to: participants(message.to()),
        cc: participants(message.cc()),
        bcc: participants(message.bcc()),
        reply_to: participants(message.reply_to()),
        sent_at: message
            .date()
            .filter(mail_parser::DateTime::is_valid)
            .map(|date| UnixTime::from_seconds(date.to_timestamp())),
        received_at,
        message_id: message.message_id().map(bracketed),
        in_reply_to: message
            .in_reply_to()
            .and_then(|list| list.first())
            .map(bracketed),
        references: message
            .references()
            .map(|list| list.iter().map(bracketed).collect())
            .unwrap_or_default(),
        list_id,
        has_list_unsubscribe: message.list_unsubscribe().is_some(),
        size_bytes,
    }
}

/// Every mailbox of an address header, group members included. A mailbox with only a name
/// (`From: Mail Delivery System`) keeps the name as unparsed text, so the sender still shows; an
/// empty one is dropped by [`Participant::new`].
fn participants(list: Option<AddressList<'_>>) -> Vec<Participant> {
    list.map(|list| {
        list.mailboxes()
            .filter_map(|mailbox| match mailbox.address() {
                Some(address) => Participant::new(mailbox.name(), address),
                None => Participant::new(None, mailbox.name()?),
            })
            .collect()
    })
    .unwrap_or_default()
}

const fn empty(received_at: UnixTime, size_bytes: u32) -> HeaderFields {
    HeaderFields {
        subject: None,
        from: Vec::new(),
        to: Vec::new(),
        cc: Vec::new(),
        bcc: Vec::new(),
        reply_to: Vec::new(),
        sent_at: None,
        received_at,
        message_id: None,
        in_reply_to: None,
        references: Vec::new(),
        list_id: None,
        has_list_unsubscribe: false,
        size_bytes,
    }
}

#[cfg(test)]
mod tests {
    use clientmail_domain::{HeaderMessageId, MessageHeader, ParticipantAddress};

    use super::*;

    const RECEIVED: UnixTime = UnixTime::from_seconds(1_700_000_000);

    fn parse(raw: &str) -> HeaderFields {
        parse_header_fields(raw.as_bytes(), RECEIVED, 2048)
    }

    fn addresses(list: &[Participant]) -> Vec<&str> {
        list.iter()
            .map(|person| person.address().as_str())
            .collect()
    }

    #[test]
    fn a_full_header_is_read() {
        let fields = parse(
            "Date: Tue, 14 Nov 2023 22:13:20 +0100\r\n\
             From: \"Lovelace, Ada\" <ada@example.com>\r\n\
             To: bob@example.com, Carol <carol@example.org>\r\n\
             Cc: dan@example.net\r\n\
             Bcc: eve@example.com\r\n\
             Reply-To: replies@example.com\r\n\
             Subject: Notes on the engine\r\n\
             Message-ID: <id-1@example.com>\r\n\
             In-Reply-To: <id-0@example.com>\r\n\
             References: <id-a@example.com> <id-0@example.com>\r\n\
             List-Id: Engines <engines.example.com>\r\n\
             List-Unsubscribe: <mailto:leave@example.com>\r\n\r\n",
        );

        assert_eq!(
            fields.subject.as_deref(),
            Some("Notes on the engine"),
            "subject"
        );
        assert_eq!(fields.from[0].name(), Some("Lovelace, Ada"), "quoted name");
        assert_eq!(
            addresses(&fields.to),
            ["bob@example.com", "carol@example.org"],
            "to"
        );
        assert_eq!(addresses(&fields.cc), ["dan@example.net"], "cc");
        assert_eq!(
            addresses(&fields.bcc),
            ["eve@example.com"],
            "bcc (sent mail)"
        );
        assert_eq!(
            addresses(&fields.reply_to),
            ["replies@example.com"],
            "reply-to"
        );
        assert_eq!(
            fields.sent_at,
            Some(UnixTime::from_seconds(1_699_996_400)),
            "date with its zone applied"
        );
        assert_eq!(fields.received_at, RECEIVED, "INTERNALDATE kept");
        assert_eq!(
            fields.message_id.as_deref(),
            Some("<id-1@example.com>"),
            "message id, brackets kept for the domain"
        );
        assert_eq!(
            fields.in_reply_to.as_deref(),
            Some("<id-0@example.com>"),
            "in-reply-to"
        );
        assert_eq!(
            fields.references,
            ["<id-a@example.com>", "<id-0@example.com>"],
            "references in order"
        );
        assert_eq!(
            fields.list_id.as_deref(),
            Some("engines.example.com"),
            "list id"
        );
        assert!(fields.has_list_unsubscribe, "unsubscribe present");
        assert_eq!(fields.size_bytes, 2048, "size kept");
    }

    #[test]
    fn encoded_words_are_decoded() {
        let fields = parse(
            "From: =?iso-8859-1?q?Andr=E9?= <andre@example.com>\r\n\
             Subject: =?utf-8?b?w6lsw6hnZQ==?= =?utf-8?q?_et_caf=C3=A9?=\r\n\r\n",
        );

        assert_eq!(fields.from[0].name(), Some("André"), "Latin-1 name");
        assert_eq!(
            fields.subject.as_deref(),
            Some("élège et café"),
            "adjacent words joined"
        );
    }

    #[test]
    fn group_members_are_flattened_and_empty_groups_add_nobody() {
        let fields =
            parse("To: Team: ada@example.com, bob@example.com;, undisclosed-recipients:;\r\n\r\n");

        assert_eq!(
            addresses(&fields.to),
            ["ada@example.com", "bob@example.com"],
            "members only"
        );
    }

    #[test]
    fn addresses_that_are_not_valid_are_kept_as_text() {
        let fields = parse("From: Printer <printer@localhost>\r\n\r\n");

        assert!(
            matches!(fields.from[0].address(), ParticipantAddress::Unparsed(_)),
            "kept as text"
        );
    }

    #[test]
    fn a_sender_with_only_a_name_is_kept_as_text() {
        let fields = parse("From: Mail Delivery System\r\n\r\n");

        assert!(
            matches!(
                fields.from.first().map(Participant::address),
                Some(ParticipantAddress::Unparsed(text)) if text == "Mail Delivery System"
            ),
            "{:?}",
            fields.from
        );
    }

    #[test]
    fn message_ids_without_an_at_sign_are_kept() {
        let header = MessageHeader::from_fields(parse(
            "Message-ID: <local-id-7>\r\n\
             In-Reply-To: <local-id-6>\r\n\
             References: <local-id-5> <local-id-6>\r\n\r\n",
        ));

        assert_eq!(
            header.message_id().map(HeaderMessageId::as_str),
            Some("local-id-7"),
            "message id"
        );
        assert_eq!(
            header.in_reply_to().map(HeaderMessageId::as_str),
            Some("local-id-6"),
            "in-reply-to"
        );
        assert_eq!(
            header
                .references()
                .iter()
                .map(HeaderMessageId::as_str)
                .collect::<Vec<_>>(),
            ["local-id-5", "local-id-6"],
            "references"
        );
    }

    #[test]
    fn dates_without_a_zone_or_out_of_range_are_dropped() {
        let no_zone = parse("Date: Tue, 14 Nov 2023 22:13:20\r\n\r\n");
        let impossible = parse("Date: Tue, 28 Feb 2023 25:61:61 +0000\r\n\r\n");

        assert_eq!(no_zone.sent_at, None, "no zone, no guess");
        assert_eq!(impossible.sent_at, None, "no date invented");
    }

    #[test]
    fn legacy_charsets_are_decoded() {
        let fields = parse("Subject: =?koi8-r?b?8NLJ18XU?=\r\n\r\n");

        assert_eq!(fields.subject.as_deref(), Some("Привет"), "KOI8-R");
    }

    #[test]
    fn missing_or_broken_headers_leave_fields_empty() {
        let empty = parse("\r\n");
        let garbage = parse_header_fields(&[0xff, 0xfe, b'\r', b'\n'], RECEIVED, 1);

        for fields in [empty, garbage] {
            assert!(
                fields.subject.is_none() && fields.from.is_empty(),
                "nothing invented"
            );
            assert_eq!(fields.received_at, RECEIVED, "INTERNALDATE kept");
            assert!(!fields.has_list_unsubscribe, "no unsubscribe");
        }
    }
}
