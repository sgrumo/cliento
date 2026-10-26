//! Opt-in test against Gmail's real IMAP server. Run with `make imap-smoke`, with
//! `CLIENTMAIL_IMAP_USER` and `CLIENTMAIL_IMAP_TOKEN` set (`docs/google-oauth.md`, section 5).
//!
//! It lists the folders, examines All Mail and fetches the newest 10 headers of the last 30 days.
//! It prints only counts, never names, addresses or subjects.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use clientmail_domain::ports::{MailConnector, MailSession};
use clientmail_domain::{
    EmailAddress, GmailItems, GmailLabel, HeaderBatch, ImapServer, MailboxListing, MailboxRole,
    RemoteHeader, SecretString, UnixTime,
};
use clientmail_imap::ImapConnector;

#[tokio::test]
#[ignore = "talks to Gmail; run with `make imap-smoke`"]
async fn gmail_signs_in_lists_folders_and_fetches_headers() {
    let (Ok(user), Ok(token)) = (
        std::env::var("CLIENTMAIL_IMAP_USER"),
        std::env::var("CLIENTMAIL_IMAP_TOKEN"),
    ) else {
        panic!("set CLIENTMAIL_IMAP_USER and CLIENTMAIL_IMAP_TOKEN; see docs/google-oauth.md");
    };
    let user = EmailAddress::parse(&user).unwrap();
    let token = SecretString::from(token);
    let connector = ImapConnector::new().unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let since = UnixTime::from_seconds(i64::try_from(now).unwrap() - 30 * 86_400);

    let mut session = connector
        .connect(&ImapServer::gmail(), &user, &token)
        .await
        .unwrap();
    let listing = session.list_mailboxes().await.unwrap();
    let all_mail = listing
        .mailboxes()
        .iter()
        .find(|mailbox| mailbox.role() == MailboxRole::AllMail)
        .expect("Gmail lists All Mail")
        .name()
        .clone();
    session.examine(&all_mail).await.unwrap();
    let found = session.search_since(since).await.unwrap();
    let newest = &found[found.len().saturating_sub(10)..];
    let batch = session.fetch_headers(newest).await.unwrap();
    session.logout().await.unwrap();

    println!("{}", folder_summary(&listing));
    println!(
        "last 30 days: {} messages; {}",
        found.len(),
        batch_summary(&batch)
    );
    for role in [
        MailboxRole::Inbox,
        MailboxRole::AllMail,
        MailboxRole::Sent,
        MailboxRole::Drafts,
        MailboxRole::Trash,
    ] {
        assert!(
            listing
                .mailboxes()
                .iter()
                .any(|mailbox| mailbox.role() == role),
            "Gmail lists a folder with role {role:?}"
        );
    }
    assert_eq!(
        batch.headers().len() + batch.skipped(),
        newest.len(),
        "every message read or counted"
    );
    assert!(
        batch
            .headers()
            .iter()
            .all(|header| header.gmail().is_some()),
        "Gmail ids on every header"
    );
}

/// How many folders have each role, and how many were skipped. No names.
fn folder_summary(listing: &MailboxListing) -> String {
    let mut roles: BTreeMap<String, usize> = BTreeMap::new();
    for mailbox in listing.mailboxes() {
        *roles.entry(format!("{:?}", mailbox.role())).or_default() += 1;
    }
    format!(
        "folders per role: {roles:?}; skipped: {}",
        listing.skipped()
    )
}

/// How many headers were fetched and skipped, and how many labels of each kind, so a system label
/// read as a user label shows up. No content.
fn batch_summary(batch: &HeaderBatch) -> String {
    let labels: Vec<&GmailLabel> = batch
        .headers()
        .iter()
        .filter_map(RemoteHeader::gmail)
        .flat_map(GmailItems::labels)
        .collect();
    let system = labels
        .iter()
        .filter(|label| matches!(label, GmailLabel::System(_)))
        .count();
    let backslash_names = labels
        .iter()
        .filter(
            |label| matches!(label, GmailLabel::Name(name) if name.as_bytes().starts_with(b"\\")),
        )
        .count();
    format!(
        "fetched {} headers, skipped {}; labels: {system} system, {} named ({backslash_names} named with a backslash)",
        batch.headers().len(),
        batch.skipped(),
        labels.len() - system,
    )
}
