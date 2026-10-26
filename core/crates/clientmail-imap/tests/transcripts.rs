//! The IMAP client against scripted servers (`tests/transcripts/`).

mod support;

use clientmail_domain::ports::MailSession;
use clientmail_domain::{
    EmailAddress, Flag, GmailLabel, ImapUid, Keyword, MailErrorKind, MailboxListing, MailboxRole,
    ModSeq, RemoteName, SecretString, SystemLabel, UnixTime,
};
use clientmail_imap::{ImapError, ImapSession};
use tokio::io::DuplexStream;

use support::{Script, TOKEN, USER};

/// Starts the script's server on one end of a pipe and returns the other end for the client.
fn serve(name: &str) -> (DuplexStream, tokio::task::JoinHandle<Result<(), String>>) {
    let (client, server) = tokio::io::duplex(64 * 1024);
    let script = Script::load(name);
    (client, tokio::spawn(script.play(server)))
}

#[expect(
    clippy::unwrap_used,
    reason = "test helper: a bad address constant is a failing test"
)]
async fn open(stream: DuplexStream) -> Result<ImapSession<DuplexStream>, ImapError> {
    let user = EmailAddress::parse(USER).unwrap();
    ImapSession::open(stream, &user, &SecretString::from(TOKEN)).await
}

/// Each listed folder as its path, role and selectability.
fn folders(listing: &MailboxListing) -> Vec<(&str, MailboxRole, bool)> {
    listing
        .mailboxes()
        .iter()
        .map(|mailbox| {
            (
                mailbox.path().as_str(),
                mailbox.role(),
                mailbox.is_selectable(),
            )
        })
        .collect()
}

#[tokio::test]
async fn gmail_signs_in_and_lists_folders_with_their_roles() {
    let (stream, server) = serve("gmail_sign_in_and_list");

    let mut session = open(stream).await.unwrap();
    let listing = session.list_mailboxes().await.unwrap();
    session.logout().await.unwrap();

    server.await.unwrap().unwrap();
    assert_eq!(
        folders(&listing),
        [
            ("INBOX", MailboxRole::Inbox, true),
            ("[Gmail]", MailboxRole::Custom, false),
            ("[Gmail]/All Mail", MailboxRole::AllMail, true),
            ("[Gmail]/Drafts", MailboxRole::Drafts, true),
            ("[Gmail]/Important", MailboxRole::Important, true),
            ("[Gmail]/Sent Mail", MailboxRole::Sent, true),
            ("[Gmail]/Spam", MailboxRole::Junk, true),
            ("[Gmail]/Starred", MailboxRole::Starred, true),
            ("[Gmail]/Trash", MailboxRole::Trash, true),
            ("Receipts", MailboxRole::Custom, true),
        ],
        "every folder, in order"
    );
    assert_eq!(listing.skipped(), 0, "nothing skipped");
}

#[tokio::test]
async fn a_refused_token_is_an_authentication_failure_after_answering_the_challenge() {
    let (stream, server) = serve("gmail_refused_token");

    let result = open(stream).await;

    // The script checks the empty answer to the challenge; a missing answer would hang it.
    server.await.unwrap().unwrap();
    let error = result.unwrap_err();
    assert_eq!(
        error.kind(),
        MailErrorKind::AuthenticationFailed,
        "{error:?}"
    );
    let shown = format!("{error} {error:?}");
    assert!(
        !shown.contains(USER) && !shown.contains(TOKEN),
        "leaked: {shown}"
    );
}

#[tokio::test]
async fn a_server_without_xoauth2_never_receives_the_token() {
    let (stream, server) = serve("no_xoauth2");

    let result = open(stream).await;

    // The script ends after the greeting and fails if the client sends anything.
    server.await.unwrap().unwrap();
    assert!(matches!(result, Err(ImapError::NoXoauth2)), "{result:?}");
}

#[tokio::test]
async fn names_are_decoded_and_unusable_ones_skipped() {
    let (stream, server) = serve("names_and_attributes");

    let mut session = open(stream).await.unwrap();
    let listing = session.list_mailboxes().await.unwrap();
    drop(session);

    server.await.unwrap().unwrap();
    assert_eq!(
        folders(&listing),
        [
            ("INBOX", MailboxRole::Inbox, true),
            ("Entwürfe", MailboxRole::Drafts, true),
            ("台北.日本語", MailboxRole::Custom, true),
            ("Clients", MailboxRole::Custom, false),
            ("Clients.Acme", MailboxRole::Custom, true),
            ("Archive", MailboxRole::Archive, true),
        ],
        "decoded, with roles"
    );
    assert_eq!(listing.skipped(), 2, "broken UTF-7 and an empty segment");
    assert_eq!(
        listing.mailboxes()[2].path().leaf_name(),
        "日本語",
        "the delimiter is kept"
    );
    assert_eq!(
        listing.mailboxes()[1].name().as_bytes(),
        b"Entw&APw-rfe",
        "the server's encoded name is kept for commands"
    );
}

#[tokio::test]
async fn a_connection_dropped_during_the_list_is_a_network_error_and_breaks_the_session() {
    let (stream, server) = serve("dropped_during_list");

    let mut session = open(stream).await.unwrap();
    let result = session.list_mailboxes().await;
    let again = session.list_mailboxes().await;

    server.await.unwrap().unwrap();
    let error = result.unwrap_err();
    assert_eq!(error.kind(), MailErrorKind::Network, "{error:?}");
    assert!(
        matches!(again, Err(ImapError::Broken)),
        "a broken session refuses further calls: {again:?}"
    );
}

#[tokio::test]
async fn a_refused_list_is_an_error_not_a_partial_list() {
    let (stream, server) = serve("list_refused");

    let mut session = open(stream).await.unwrap();
    let result = session.list_mailboxes().await;
    drop(session);

    server.await.unwrap().unwrap();
    let error = result.unwrap_err();
    assert_eq!(
        error.kind(),
        MailErrorKind::Unavailable,
        "[UNAVAILABLE] means busy: {error:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn a_silent_server_times_out() {
    let (stream, _server) = serve("silent_after_authenticate");

    // Tokio's paused clock jumps ahead while every task waits, so the 60 s timeout passes at once.
    let result = open(stream).await;

    assert!(matches!(result, Err(ImapError::Timeout)), "{result:?}");
}

#[tokio::test]
async fn names_need_the_grammar_done_right() {
    let (stream, server) = serve("tricky_names");

    let mut session = open(stream).await.unwrap();
    let listing = session.list_mailboxes().await.unwrap();
    drop(session);

    server.await.unwrap().unwrap();
    let folders: Vec<(&str, Option<char>, MailboxRole)> = listing
        .mailboxes()
        .iter()
        .map(|mailbox| {
            (
                mailbox.path().as_str(),
                mailbox.path().delimiter(),
                mailbox.role(),
            )
        })
        .collect();
    assert_eq!(
        folders,
        [
            ("a\"b\\c", Some('/'), MailboxRole::Custom),
            ("Mail\\Clients", Some('\\'), MailboxRole::Custom),
            ("Literal", Some('/'), MailboxRole::Custom),
            ("NotAllMail", Some('/'), MailboxRole::Custom),
            ("NotSent", Some('/'), MailboxRole::Custom),
            ("Entwürfe", Some('/'), MailboxRole::Custom),
        ],
        "escapes undone, literals read, no prefix roles"
    );
    assert_eq!(
        listing.skipped(),
        2,
        "the non-UTF-8 name, and the second name for Entwürfe"
    );
}

#[tokio::test]
async fn a_server_without_sasl_ir_gets_the_credentials_after_its_continuation() {
    let (stream, server) = serve("no_sasl_ir");

    let session = open(stream).await;
    let opened = session.is_ok();
    drop(session);

    server.await.unwrap().unwrap();
    assert!(opened, "signed in after the continuation");
}

#[tokio::test]
async fn too_many_connections_is_unavailable_not_a_bad_token() {
    let (stream, server) = serve("gmail_too_many_connections");

    let result = open(stream).await;

    server.await.unwrap().unwrap();
    assert!(matches!(result, Err(ImapError::Unavailable)), "{result:?}");
}

#[tokio::test]
async fn a_bye_greeting_is_unavailable() {
    let (stream, server) = serve("greeting_bye");

    let result = open(stream).await;

    server.await.unwrap().unwrap();
    assert!(matches!(result, Err(ImapError::Unavailable)), "{result:?}");
}

#[tokio::test]
async fn a_preauth_greeting_is_refused() {
    let (stream, server) = serve("greeting_preauth");

    let result = open(stream).await;

    server.await.unwrap().unwrap();
    assert!(matches!(result, Err(ImapError::Protocol(_))), "{result:?}");
}

#[tokio::test]
async fn a_refused_capability_is_a_protocol_error() {
    let (stream, server) = serve("capability_refused");

    let result = open(stream).await;

    server.await.unwrap().unwrap();
    assert!(matches!(result, Err(ImapError::Protocol(_))), "{result:?}");
}

/// 2026-10-26 12:00 UTC, thirty days before which the searches start.
const SINCE: UnixTime = UnixTime::from_seconds(1_793_016_000);

fn uids(values: &[u32]) -> Vec<ImapUid> {
    values
        .iter()
        .filter_map(|value| ImapUid::new(*value))
        .collect()
}

#[tokio::test]
async fn gmail_examines_all_mail_searches_and_fetches_ids_and_labels() {
    let (stream, server) = serve("gmail_fetch_headers");

    let mut session = open(stream).await.unwrap();
    let gmail = session.has_gmail_extensions();
    let all_mail = session
        .examine(&RemoteName::new(b"[Gmail]/All Mail".to_vec()))
        .await
        .unwrap();
    let found = session
        .search_since(SINCE.saturating_add_seconds(-30 * 86_400))
        .await
        .unwrap();
    let batch = session.fetch_headers(&found).await.unwrap();
    drop(session);

    server.await.unwrap().unwrap();
    assert!(gmail, "X-GM-EXT-1 after sign-in");
    assert_eq!(all_mail.uid_validity().get(), 11, "UIDVALIDITY");
    assert_eq!(
        all_mail.uid_next().map(ImapUid::get),
        Some(98_765),
        "UIDNEXT"
    );
    assert_eq!(
        all_mail.highest_modseq().map(ModSeq::get),
        Some(5_550_000),
        "HIGHESTMODSEQ"
    );
    assert_eq!(all_mail.exists(), 1234, "EXISTS");
    assert_eq!(found, uids(&[101, 102, 103, 105]), "sorted UIDs");
    assert_eq!(
        batch.skipped(),
        2,
        "the unparsable line and the message without Gmail ids"
    );
    let [first, second] = batch.headers() else {
        panic!("two headers, got {}", batch.headers().len());
    };
    assert_eq!(first.uid().get(), 101, "All Mail UID");
    assert!(first.flags().contains(Flag::Seen), "flags");
    assert_eq!(first.header().subject(), Some("Engine notes"), "header");
    let gmail = first.gmail().unwrap();
    assert_eq!(
        gmail.message_id().get(),
        1_278_455_344_230_334_865,
        "X-GM-MSGID"
    );
    assert_eq!(
        gmail.thread_id().get(),
        1_278_455_344_230_334_865,
        "X-GM-THRID"
    );
    assert_eq!(
        gmail.labels(),
        [
            GmailLabel::System(SystemLabel::Inbox),
            GmailLabel::System(SystemLabel::Important),
            GmailLabel::Name(RemoteName::new(b"Work".to_vec())),
        ],
        "system labels and an atom"
    );
    assert_eq!(
        second.gmail().unwrap().labels(),
        [
            GmailLabel::Name(RemoteName::new(b"Clients/\"Acme\"".to_vec())),
            GmailLabel::Name(RemoteName::new(b"Receipts 2026".to_vec())),
        ],
        "a quoted label with escapes undone, and a literal"
    );
    assert_eq!(
        second.header().received_at().seconds(),
        1_793_001_600,
        "INTERNALDATE with its zone applied"
    );
}

#[tokio::test]
async fn a_plain_server_fetches_without_gmail_items() {
    let (stream, server) = serve("plain_fetch_headers");

    let mut session = open(stream).await.unwrap();
    let gmail = session.has_gmail_extensions();
    let inbox = session
        .examine(&RemoteName::new(b"INBOX".to_vec()))
        .await
        .unwrap();
    let found = session
        .search_since(SINCE.saturating_add_seconds(-30 * 86_400))
        .await
        .unwrap();
    let batch = session.fetch_headers(&found).await.unwrap();
    drop(session);

    server.await.unwrap().unwrap();
    assert!(!gmail, "no X-GM-EXT-1");
    assert_eq!(inbox.uid_validity().get(), 1_700_000_000, "UIDVALIDITY");
    assert_eq!(inbox.highest_modseq(), None, "no HIGHESTMODSEQ sent");
    assert_eq!(batch.skipped(), 0, "nothing skipped");
    let [first, second] = batch.headers() else {
        panic!("two headers");
    };
    assert!(first.flags().contains(Flag::Forwarded), "$Forwarded");
    assert_eq!(
        first
            .flags()
            .keywords()
            .map(Keyword::as_str)
            .collect::<Vec<_>>(),
        ["Work"],
        "keyword"
    );
    assert_eq!(first.header().subject(), Some("Café"), "encoded subject");
    assert!(first.gmail().is_none(), "no Gmail items");
    assert!(second.flags().contains(Flag::Answered), "\\Answered");
    assert_eq!(
        second.header().list_id(),
        Some("news.example.com"),
        "List-Id"
    );
    assert!(second.header().has_list_unsubscribe(), "List-Unsubscribe");
}

#[tokio::test]
async fn a_folder_without_uidvalidity_is_refused() {
    let (stream, server) = serve("examine_without_uidvalidity");

    let mut session = open(stream).await.unwrap();
    let result = session.examine(&RemoteName::new(b"INBOX".to_vec())).await;
    drop(session);

    server.await.unwrap().unwrap();
    assert!(matches!(result, Err(ImapError::Protocol(_))), "{result:?}");
}

#[tokio::test]
async fn a_busy_fetch_is_unavailable() {
    let (stream, server) = serve("fetch_unavailable");

    let mut session = open(stream).await.unwrap();
    let result = session.fetch_headers(&uids(&[1])).await;
    let logout = session.logout().await;

    server.await.unwrap().unwrap();
    assert!(matches!(result, Err(ImapError::Unavailable)), "{result:?}");
    assert!(
        logout.is_ok(),
        "a refusal leaves the session usable: {logout:?}"
    );
}

#[tokio::test]
async fn a_connection_dropped_during_a_fetch_breaks_the_session() {
    let (stream, server) = serve("dropped_during_fetch");

    let mut session = open(stream).await.unwrap();
    let result = session.fetch_headers(&uids(&[1])).await;
    let again = session.fetch_headers(&uids(&[1])).await;

    server.await.unwrap().unwrap();
    assert!(matches!(result, Err(ImapError::Network)), "{result:?}");
    assert!(matches!(again, Err(ImapError::Broken)), "{again:?}");
}

#[tokio::test]
async fn fetching_nothing_sends_nothing() {
    let (stream, server) = serve("no_sasl_ir");

    let mut session = open(stream).await.unwrap();
    let batch = session.fetch_headers(&[]).await.unwrap();
    drop(session);

    server.await.unwrap().unwrap();
    assert!(batch.headers().is_empty() && batch.skipped() == 0, "empty");
}
