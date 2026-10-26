use std::time::{Duration, Instant};

use clientmail_domain::ports::{AccountRepository, MailboxRepository, PageCursor};
use clientmail_domain::{
    AccountId, Flag, GmailMessageId, GmailThreadId, HeaderFields, ImapUid, Keyword, Mailbox,
    MailboxPath, MailboxRole, MessageFlags, MessageHeader, NewAccount, Participant,
    RemoteMessageRef, UidValidity, UnixTime,
};

use super::*;

struct Fixture {
    store: SqliteStore,
    account: AccountId,
    folder: MailboxId,
    other_folder: MailboxId,
    label: MailboxId,
}

fn fixture() -> Fixture {
    let store = SqliteStore::open_in_memory().unwrap();
    let account = NewAccount::parse("ada@example.com", None)
        .unwrap()
        .into_account();
    store.insert(&account).unwrap();
    let mailbox = |path: &str, role| {
        let mailbox = Mailbox::new(
            MailboxId::generate(),
            account.id(),
            MailboxPath::new(path, Some('/')).unwrap(),
            role,
            true,
        );
        store.upsert_mailbox(&mailbox).unwrap();
        mailbox.id()
    };
    Fixture {
        folder: mailbox("INBOX", MailboxRole::Inbox),
        other_folder: mailbox("Archive", MailboxRole::Archive),
        label: mailbox("Work", MailboxRole::Custom),
        account: account.id(),
        store,
    }
}

fn header(received_at: i64) -> MessageHeader {
    MessageHeader::from_fields(HeaderFields {
        subject: Some("Quarterly report".to_owned()),
        from: Participant::new(Some("Ada"), "ada@example.com")
            .into_iter()
            .collect(),
        to: [
            Participant::new(None, "bob@example.com"),
            Participant::new(Some("Group"), "undisclosed-recipients:;"),
        ]
        .into_iter()
        .flatten()
        .collect(),
        cc: Participant::new(None, "carol@example.com")
            .into_iter()
            .collect(),
        bcc: Participant::new(None, "dan@example.com")
            .into_iter()
            .collect(),
        reply_to: Participant::new(None, "reply@example.com")
            .into_iter()
            .collect(),
        sent_at: Some(UnixTime::from_seconds(received_at - 60)),
        received_at: UnixTime::from_seconds(received_at),
        message_id: Some("<report@example.com>".to_owned()),
        in_reply_to: Some("<parent@example.com>".to_owned()),
        references: vec!["<root@example.com> <parent@example.com>".to_owned()],
        list_id: Some("Reports <reports.example.com>".to_owned()),
        has_list_unsubscribe: true,
        size_bytes: 2_048,
    })
}

fn flags() -> MessageFlags {
    let mut flags = MessageFlags::default();
    flags.insert(Flag::Seen);
    flags.insert(Flag::Forwarded);
    flags.insert_keyword(Keyword::parse("$Junk").unwrap());
    flags
}

fn imap_ref(mailbox: MailboxId, uid_validity: u32, uid: u32) -> RemoteMessageRef {
    RemoteMessageRef::Imap {
        mailbox,
        uid_validity: UidValidity::new(uid_validity).unwrap(),
        uid: ImapUid::new(uid).unwrap(),
    }
}

fn gmail_ref(message_id: u64, all_mail_uid: u32) -> RemoteMessageRef {
    RemoteMessageRef::Gmail {
        message_id: GmailMessageId::new(message_id),
        thread_id: GmailThreadId::new(message_id),
        all_mail_uid_validity: UidValidity::new(7).unwrap(),
        all_mail_uid: ImapUid::new(all_mail_uid).unwrap(),
    }
}

fn message(fixture: &Fixture, remote: RemoteMessageRef, received_at: i64) -> Message {
    Message::new(
        MessageId::generate(),
        fixture.account,
        remote,
        header(received_at),
        flags(),
    )
}

fn imap_message(fixture: &Fixture, uid: u32, received_at: i64) -> Message {
    message(fixture, imap_ref(fixture.folder, 7, uid), received_at)
}

fn gmail_message(fixture: &Fixture, message_id: u64, received_at: i64) -> Message {
    message(fixture, gmail_ref(message_id, 1), received_at)
}

fn page(
    fixture: &Fixture,
    mailbox: MailboxId,
    limit: u32,
    after: Option<&Message>,
) -> Vec<Message> {
    fixture
        .store
        .page(&PageQuery {
            mailbox,
            limit,
            before: after.map(|last| PageCursor {
                received_at: last.header().received_at(),
                id: last.id(),
            }),
        })
        .unwrap()
}

fn times(page: &[Message]) -> Vec<i64> {
    page.iter()
        .map(|message| message.header().received_at().seconds())
        .collect()
}

fn count(store: &SqliteStore, table: &str) -> i64 {
    store
        .with_connection(|connection| {
            Ok(
                connection.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                    row.get(0)
                })?,
            )
        })
        .unwrap()
}

#[test]
fn upsert_then_read_returns_an_equal_message() {
    let fixture = fixture();
    let message = imap_message(&fixture, 1, 1_000);

    let ids = fixture
        .store
        .upsert_messages(std::slice::from_ref(&message))
        .unwrap();
    let read = fixture.store.message(message.id()).unwrap();

    assert_eq!(ids, [message.id()], "new message keeps its id");
    assert_eq!(read, Some(message), "every field survives the round trip");
}

#[test]
fn upsert_of_a_known_imap_key_keeps_the_stored_id_and_replaces_its_state() {
    let fixture = fixture();
    let first = imap_message(&fixture, 1, 1_000);
    fixture
        .store
        .upsert_messages(std::slice::from_ref(&first))
        .unwrap();
    let mut new_flags = MessageFlags::default();
    new_flags.insert(Flag::Flagged);
    let again = Message::new(
        MessageId::generate(),
        fixture.account,
        first.remote(),
        header(1_000),
        new_flags.clone(),
    );

    let ids = fixture.store.upsert_messages(&[again]).unwrap();
    let read = fixture.store.message(first.id()).unwrap().unwrap();

    assert_eq!(ids, [first.id()], "stored id wins over the new one");
    assert_eq!(read.flags(), &new_flags, "flags and keywords replaced");
    assert_eq!(count(&fixture.store, "messages"), 1, "no duplicate row");
}

#[test]
fn upsert_of_a_known_gmail_key_updates_its_all_mail_uid_and_thread() {
    let fixture = fixture();
    let first = gmail_message(&fixture, 42, 1_000);
    fixture
        .store
        .upsert_messages(std::slice::from_ref(&first))
        .unwrap();
    let moved = RemoteMessageRef::Gmail {
        message_id: GmailMessageId::new(42),
        thread_id: GmailThreadId::new(99),
        all_mail_uid_validity: UidValidity::new(8).unwrap(),
        all_mail_uid: ImapUid::new(500).unwrap(),
    };
    let again = message(&fixture, moved, 1_000);

    let ids = fixture.store.upsert_messages(&[again]).unwrap();
    let read = fixture.store.message(first.id()).unwrap().unwrap();

    assert_eq!(ids, [first.id()], "stored id kept");
    assert_eq!(read.remote(), moved, "uid, uidvalidity and thread updated");
}

#[test]
fn same_server_key_twice_in_one_batch_is_one_message() {
    let fixture = fixture();
    let first = imap_message(&fixture, 1, 1_000);
    let duplicate = imap_message(&fixture, 1, 1_000);

    let ids = fixture
        .store
        .upsert_messages(&[first.clone(), duplicate])
        .unwrap();

    assert_eq!(
        ids,
        [first.id(), first.id()],
        "both resolve to the first id"
    );
    assert_eq!(count(&fixture.store, "messages"), 1, "one row");
}

#[test]
fn rekeying_after_a_uidvalidity_change_keeps_the_id() {
    let fixture = fixture();
    let original = imap_message(&fixture, 1, 1_000);
    fixture
        .store
        .upsert_messages(std::slice::from_ref(&original))
        .unwrap();
    let rekeyed = Message::new(
        original.id(),
        fixture.account,
        imap_ref(fixture.folder, 8, 31),
        header(1_000),
        flags(),
    );
    let unrelated = imap_message(&fixture, 2, 2_000);

    let ids = fixture
        .store
        .upsert_messages(&[unrelated.clone(), rekeyed.clone()])
        .unwrap();
    let read = fixture.store.message(original.id()).unwrap();

    assert_eq!(ids, [unrelated.id(), original.id()], "ids kept");
    assert_eq!(read, Some(rekeyed), "new server key stored");
    assert_eq!(count(&fixture.store, "messages"), 2, "no extra row");
}

#[test]
fn moving_an_imap_message_keeps_its_id_and_changes_its_folder() {
    let fixture = fixture();
    let original = imap_message(&fixture, 1, 1_000);
    fixture
        .store
        .upsert_messages(std::slice::from_ref(&original))
        .unwrap();
    let moved = Message::new(
        original.id(),
        fixture.account,
        imap_ref(fixture.other_folder, 3, 9),
        header(1_000),
        flags(),
    );

    fixture
        .store
        .upsert_messages(std::slice::from_ref(&moved))
        .unwrap();

    assert!(
        page(&fixture, fixture.folder, 10, None).is_empty(),
        "left INBOX"
    );
    assert_eq!(
        page(&fixture, fixture.other_folder, 10, None),
        vec![moved],
        "now in Archive with the same id"
    );
}

#[test]
fn gmail_ids_round_trip_across_the_full_u64_range() {
    let fixture = fixture();
    let messages = [u64::MAX, 1 << 63].map(|id| gmail_message(&fixture, id, 1_000));

    fixture.store.upsert_messages(&messages).unwrap();

    for message in messages {
        let read = fixture.store.message(message.id()).unwrap();
        assert_eq!(read, Some(message), "u64 bits kept");
    }
}

#[test]
fn schema_rejects_half_filled_remote_references() {
    let fixture = fixture();
    let insert = |kind: &str, mailbox: Option<String>, gmail_id: Option<i64>| {
        fixture.store.with_connection(|connection| {
            Ok(connection.execute(
                "INSERT INTO messages (id, account_id, remote_kind, mailbox_id, uid_validity, \
                 uid, gmail_message_id, gmail_thread_id, received_at, has_list_unsubscribe, \
                 size_bytes, flags) VALUES (?1, ?2, ?3, ?4, 1, 1, ?5, ?5, 0, 0, 0, 0)",
                params![
                    MessageId::generate().to_string(),
                    fixture.account.to_string(),
                    kind,
                    mailbox,
                    gmail_id,
                ],
            )?)
        })
    };

    let imap_without_mailbox = insert("imap", None, None);
    let gmail_without_ids = insert("gmail", None, None);
    let gmail_with_mailbox = insert("gmail", Some(fixture.folder.to_string()), Some(1));

    assert!(imap_without_mailbox.is_err(), "imap needs a mailbox");
    assert!(gmail_without_ids.is_err(), "gmail needs its ids");
    assert!(gmail_with_mailbox.is_err(), "gmail must not have a mailbox");
}

#[test]
fn failed_batch_stores_nothing() {
    let fixture = fixture();
    let good = imap_message(&fixture, 1, 1_000);
    let orphan = message(&fixture, imap_ref(MailboxId::generate(), 7, 2), 1_000);

    let result = fixture.store.upsert_messages(&[good, orphan]);

    assert!(result.is_err(), "unknown mailbox violates the foreign key");
    assert_eq!(
        count(&fixture.store, "messages"),
        0,
        "transaction rolled back"
    );
    assert_eq!(
        count(&fixture.store, "message_participants"),
        0,
        "no child rows"
    );
}

#[test]
fn page_lists_newest_first_and_continues_after_a_cursor() {
    let fixture = fixture();
    let messages: Vec<Message> = (1..=5)
        .map(|uid| imap_message(&fixture, uid, i64::from(uid) * 100))
        .collect();
    fixture.store.upsert_messages(&messages).unwrap();

    let first = page(&fixture, fixture.folder, 2, None);
    let second = page(&fixture, fixture.folder, 2, first.last());

    assert_eq!(times(&first), [500, 400], "newest two");
    assert_eq!(times(&second), [300, 200], "the next two");
}

#[test]
fn page_breaks_ties_on_equal_times_by_id() {
    let fixture = fixture();
    let messages: Vec<Message> = (1..=3)
        .map(|uid| imap_message(&fixture, uid, 1_000))
        .collect();
    fixture.store.upsert_messages(&messages).unwrap();
    let mut expected: Vec<MessageId> = messages.iter().map(Message::id).collect();
    expected.sort_unstable_by(|left, right| right.cmp(left));

    let first = page(&fixture, fixture.folder, 2, None);
    let rest = page(&fixture, fixture.folder, 2, first.last());

    let listed: Vec<MessageId> = first.iter().chain(&rest).map(Message::id).collect();
    assert_eq!(listed, expected, "every message once, highest id first");
}

#[test]
fn page_of_one_folder_leaves_out_other_folders() {
    let fixture = fixture();
    let inbox = imap_message(&fixture, 1, 1_000);
    let archived = message(&fixture, imap_ref(fixture.other_folder, 7, 1), 2_000);
    fixture
        .store
        .upsert_messages(&[inbox.clone(), archived])
        .unwrap();

    let listed = page(&fixture, fixture.folder, 10, None);

    assert_eq!(listed, vec![inbox], "only INBOX messages");
}

#[test]
fn page_skips_deleted_messages_without_coming_back_short() {
    let fixture = fixture();
    let kept: Vec<Message> = (1..=3)
        .map(|uid| imap_message(&fixture, uid, i64::from(uid)))
        .collect();
    let mut deleted_flags = flags();
    deleted_flags.insert(Flag::Deleted);
    let deleted: Vec<Message> = (10..=12)
        .map(|uid| {
            Message::new(
                MessageId::generate(),
                fixture.account,
                imap_ref(fixture.folder, 7, uid),
                header(i64::from(uid) * 100),
                deleted_flags.clone(),
            )
        })
        .collect();
    fixture.store.upsert_messages(&kept).unwrap();
    fixture.store.upsert_messages(&deleted).unwrap();

    let listed = page(&fixture, fixture.folder, 2, None);

    assert_eq!(times(&listed), [3, 2], "newest kept messages fill the page");
}

#[test]
fn page_lists_gmail_messages_by_label_and_continues_after_a_cursor() {
    let fixture = fixture();
    let labelled: Vec<Message> = (1..=3)
        .map(|id| gmail_message(&fixture, id, i64::try_from(id).unwrap() * 100))
        .collect();
    let unlabelled = gmail_message(&fixture, 9, 5_000);
    fixture.store.upsert_messages(&labelled).unwrap();
    fixture.store.upsert_messages(&[unlabelled]).unwrap();
    for message in &labelled {
        fixture
            .store
            .set_labels(message.id(), &[fixture.label])
            .unwrap();
    }

    let first = page(&fixture, fixture.label, 2, None);
    let second = page(&fixture, fixture.label, 2, first.last());

    assert_eq!(times(&first), [300, 200], "newest labelled two");
    assert_eq!(times(&second), [100], "the rest");
}

#[test]
fn label_page_follows_a_changed_received_time() {
    let fixture = fixture();
    let older = gmail_message(&fixture, 1, 100);
    fixture
        .store
        .upsert_messages(std::slice::from_ref(&older))
        .unwrap();
    fixture
        .store
        .set_labels(older.id(), &[fixture.label])
        .unwrap();
    let bumped = Message::new(
        older.id(),
        fixture.account,
        older.remote(),
        header(900),
        flags(),
    );

    fixture
        .store
        .upsert_messages(std::slice::from_ref(&bumped))
        .unwrap();
    let listed = page(&fixture, fixture.label, 10, None);

    assert_eq!(times(&listed), [900], "label row carries the new time");
}

#[test]
fn page_queries_read_in_index_order() {
    let fixture = fixture();
    let plans: Vec<String> = [
        "messages m WHERE m.mailbox_id = ?1 AND (m.received_at, m.id) < (?2, ?3) \
         ORDER BY m.received_at DESC, m.id DESC LIMIT ?4",
        "message_labels l JOIN messages m ON m.id = l.message_id WHERE l.mailbox_id = ?1 \
         AND (l.received_at, l.message_id) < (?2, ?3) \
         ORDER BY l.received_at DESC, l.message_id DESC LIMIT ?4",
    ]
    .iter()
    .map(|from| {
        fixture
            .store
            .with_connection(|connection| {
                let mut statement =
                    connection.prepare(&format!("EXPLAIN QUERY PLAN SELECT m.id FROM {from}"))?;
                let details = statement
                    .query_map(params!["a", 0, "b", 1], |row| row.get::<_, String>(3))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(details.join("; "))
            })
            .unwrap()
    })
    .collect();

    for plan in plans {
        assert!(!plan.contains("TEMP B-TREE"), "no sort step: {plan}");
        assert!(plan.contains('<'), "the cursor seeks in the index: {plan}");
    }
}

#[test]
fn set_labels_rejects_imap_and_unknown_messages() {
    let fixture = fixture();
    let imap = imap_message(&fixture, 1, 1_000);
    fixture
        .store
        .upsert_messages(std::slice::from_ref(&imap))
        .unwrap();

    let on_imap = fixture.store.set_labels(imap.id(), &[fixture.folder]);
    let on_missing = fixture
        .store
        .set_labels(MessageId::generate(), &[fixture.label]);

    assert!(
        matches!(on_imap, Err(StoreError::LabelsOnImapMessage)),
        "imap folder is its server reference, got {on_imap:?}"
    );
    assert!(
        matches!(on_missing, Err(StoreError::MissingMessage)),
        "unknown message, got {on_missing:?}"
    );
    assert_eq!(
        count(&fixture.store, "message_labels"),
        0,
        "nothing written"
    );
}

#[test]
fn deleting_a_mailbox_removes_its_imap_messages_and_labels() {
    let fixture = fixture();
    let imap = imap_message(&fixture, 1, 1_000);
    let gmail = gmail_message(&fixture, 1, 1_000);
    fixture
        .store
        .upsert_messages(&[imap, gmail.clone()])
        .unwrap();
    fixture
        .store
        .set_labels(gmail.id(), &[fixture.folder])
        .unwrap();

    fixture.store.delete_mailbox(fixture.folder).unwrap();

    assert_eq!(count(&fixture.store, "messages"), 1, "gmail message kept");
    assert_eq!(count(&fixture.store, "message_labels"), 0, "label removed");
}

#[test]
fn deleting_the_account_removes_all_its_mail() {
    let fixture = fixture();
    let imap = imap_message(&fixture, 1, 1_000);
    let gmail = gmail_message(&fixture, 1, 1_000);
    fixture
        .store
        .upsert_messages(&[imap, gmail.clone()])
        .unwrap();
    fixture
        .store
        .set_labels(gmail.id(), &[fixture.label])
        .unwrap();

    fixture.store.delete(fixture.account).unwrap();

    for table in [
        "mailboxes",
        "messages",
        "message_participants",
        "message_references",
        "message_keywords",
        "message_labels",
    ] {
        assert_eq!(count(&fixture.store, table), 0, "{table} emptied");
    }
}

#[test]
#[ignore = "performance budget; run with `cargo test --release -- --ignored`"]
fn upserting_ten_thousand_messages_takes_under_two_seconds() {
    let fixture = fixture();
    let messages: Vec<Message> = (1..=10_000)
        .map(|uid| imap_message(&fixture, uid, i64::from(uid)))
        .collect();

    let started = Instant::now();
    fixture.store.upsert_messages(&messages).unwrap();
    let elapsed = started.elapsed();

    assert!(
        elapsed < Duration::from_secs(2),
        "10,000 upserts took {elapsed:?}"
    );
}
