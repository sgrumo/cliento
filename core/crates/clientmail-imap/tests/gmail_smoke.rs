//! Opt-in test against Gmail's real IMAP server. Run with `make imap-smoke`, with
//! `CLIENTMAIL_IMAP_USER` and `CLIENTMAIL_IMAP_TOKEN` set (`docs/google-oauth.md`, section 5).
//!
//! It prints how many folders have each role, never their names.

use std::collections::BTreeMap;

use clientmail_domain::ports::{MailConnector, MailSession};
use clientmail_domain::{EmailAddress, ImapServer, MailboxRole, SecretString};
use clientmail_imap::ImapConnector;

#[tokio::test]
#[ignore = "talks to Gmail; run with `make imap-smoke`"]
async fn gmail_signs_in_and_lists_folders() {
    let (Ok(user), Ok(token)) = (
        std::env::var("CLIENTMAIL_IMAP_USER"),
        std::env::var("CLIENTMAIL_IMAP_TOKEN"),
    ) else {
        panic!("set CLIENTMAIL_IMAP_USER and CLIENTMAIL_IMAP_TOKEN; see docs/google-oauth.md");
    };
    let user = EmailAddress::parse(&user).unwrap();
    let token = SecretString::from(token);
    let connector = ImapConnector::new().unwrap();

    let mut session = connector
        .connect(&ImapServer::gmail(), &user, &token)
        .await
        .unwrap();
    let listing = session.list_mailboxes().await.unwrap();
    session.logout().await.unwrap();

    let mut roles: BTreeMap<String, usize> = BTreeMap::new();
    for mailbox in listing.mailboxes() {
        *roles.entry(format!("{:?}", mailbox.role())).or_default() += 1;
    }
    println!(
        "folders per role: {roles:?}; skipped: {}",
        listing.skipped()
    );
    let has = |role: MailboxRole| {
        listing
            .mailboxes()
            .iter()
            .any(|mailbox| mailbox.role() == role)
    };
    for role in [
        MailboxRole::Inbox,
        MailboxRole::AllMail,
        MailboxRole::Sent,
        MailboxRole::Drafts,
        MailboxRole::Trash,
    ] {
        assert!(has(role), "Gmail lists a folder with role {role:?}");
    }
}
