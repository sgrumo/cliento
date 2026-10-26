//! Domain types, validation and port traits of the `ClientMail` core.
//!
//! This crate performs no I/O. Storage and network crates implement the traits in [`ports`].
//!
//! Types that hold personal data (names, addresses, subjects, folder names, message ids) print
//! `<redacted>` in their `Debug` output, so logging them never leaks that data.

mod account;
mod address;
mod blob;
mod error;
mod flags;
mod id;
mod imap;
mod mailbox;
mod message;
mod oauth;
mod participant;
pub mod ports;
mod provider;
mod redact;
mod secret;
mod sync;
mod time;

pub use account::{Account, AccountId, NewAccount};
pub use address::EmailAddress;
pub use blob::BlobId;
pub use error::DomainError;
pub use flags::{Flag, Keyword, MessageFlags};
pub use imap::{ImapServer, MailErrorKind, MailboxListing, RemoteMailbox, RemoteName};
pub use mailbox::{Mailbox, MailboxId, MailboxPath, MailboxRole};
pub use message::{
    GmailMessageId, GmailThreadId, HeaderFields, HeaderMessageId, ImapUid, Message, MessageHeader,
    MessageId, RemoteMessageRef, UidValidity,
};
pub use oauth::{
    AccessToken, AuthorizationCode, AuthorizationRequest, OAuthErrorKind, OAuthIdentity,
    OAuthState, PkceVerifier, TokenGrant,
};
pub use participant::{Participant, ParticipantAddress};
pub use provider::Provider;
/// Re-exported so crates that implement or call [`ports::SecretStore`] use the same secret types.
pub use secrecy::{ExposeSecret, SecretString};
pub use secret::{SecretKey, SecretKind};
pub use sync::{MailboxSyncState, ModSeq};
pub use time::UnixTime;
