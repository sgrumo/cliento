use std::fmt;

use crate::redact::Redacted;
use crate::{
    GmailMessageId, GmailThreadId, ImapUid, MailboxPath, MailboxRole, MessageFlags, MessageHeader,
    ModSeq, UidValidity,
};

/// Where an IMAP server listens. Only implicit TLS (the connection starts encrypted) is supported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImapServer {
    host: String,
    port: u16,
}

impl ImapServer {
    /// Creates a server address.
    ///
    /// # Arguments
    ///
    /// * `host` - The host name, also checked against the server's certificate.
    /// * `port` - The TLS port, usually 993.
    #[must_use]
    pub const fn new(host: String, port: u16) -> Self {
        Self { host, port }
    }

    /// Returns Gmail's IMAP server, `imap.gmail.com:993`.
    #[must_use]
    pub fn gmail() -> Self {
        Self::new("imap.gmail.com".to_owned(), 993)
    }

    /// Returns the host name.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Returns the port.
    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }
}

/// A folder's name exactly as the server sent it: the bytes to send back when selecting it.
///
/// [`MailboxPath`] holds the decoded name for people and for storage; decoding is not always
/// reversible, so commands use this instead. `Debug` hides it.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct RemoteName(Vec<u8>);

impl RemoteName {
    /// Wraps the server's bytes.
    ///
    /// # Arguments
    ///
    /// * `bytes` - The name as listed, after the protocol's own quoting is undone.
    #[must_use]
    pub const fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// Returns the server's bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for RemoteName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("RemoteName")
            .field(&Redacted)
            .finish()
    }
}

/// A folder as the server lists it, before it has a local id. `Debug` hides its names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteMailbox {
    name: RemoteName,
    path: MailboxPath,
    role: MailboxRole,
    selectable: bool,
}

impl RemoteMailbox {
    /// Creates a listed folder.
    ///
    /// # Arguments
    ///
    /// * `name` - The name as the server sent it, for commands.
    /// * `path` - The decoded name and delimiter, for people and storage.
    /// * `role` - What the folder is for, from its name and attributes.
    /// * `selectable` - Whether it can hold messages (`\Noselect` folders only group others).
    #[must_use]
    pub const fn new(
        name: RemoteName,
        path: MailboxPath,
        role: MailboxRole,
        selectable: bool,
    ) -> Self {
        Self {
            name,
            path,
            role,
            selectable,
        }
    }

    /// Returns the name as the server sent it.
    #[must_use]
    pub const fn name(&self) -> &RemoteName {
        &self.name
    }

    /// Returns the folder's path.
    #[must_use]
    pub const fn path(&self) -> &MailboxPath {
        &self.path
    }

    /// Returns what the folder is for.
    #[must_use]
    pub const fn role(&self) -> MailboxRole {
        self.role
    }

    /// Returns whether the folder can hold messages.
    #[must_use]
    pub const fn is_selectable(&self) -> bool {
        self.selectable
    }
}

/// A server's folders, and how many it listed that cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailboxListing {
    mailboxes: Vec<RemoteMailbox>,
    skipped: usize,
}

impl MailboxListing {
    /// Creates a listing.
    ///
    /// # Arguments
    ///
    /// * `mailboxes` - The usable folders, in the server's order. No two have the same path.
    /// * `skipped` - How many listed folders were left out: a name that is invalid, cannot be
    ///   parsed, or decodes to the path of a folder listed before it. Sync carries on without
    ///   them.
    #[must_use]
    pub const fn new(mailboxes: Vec<RemoteMailbox>, skipped: usize) -> Self {
        Self { mailboxes, skipped }
    }

    /// Returns the usable folders.
    #[must_use]
    pub fn mailboxes(&self) -> &[RemoteMailbox] {
        &self.mailboxes
    }

    /// Returns how many listed folders were left out.
    #[must_use]
    pub const fn skipped(&self) -> usize {
        self.skipped
    }
}

/// A folder opened for reading: where sync stands in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectedMailbox {
    uid_validity: UidValidity,
    uid_next: Option<ImapUid>,
    highest_modseq: Option<ModSeq>,
    exists: u32,
}

impl SelectedMailbox {
    /// Creates the state of an opened folder.
    ///
    /// # Arguments
    ///
    /// * `uid_validity` - The folder's `UIDVALIDITY`; a new value invalidates every stored `UID`.
    /// * `uid_next` - The `UID` the next message will get, when the server said.
    /// * `highest_modseq` - The `HIGHESTMODSEQ`, when the server supports `CONDSTORE`.
    /// * `exists` - How many messages the folder holds.
    #[must_use]
    pub const fn new(
        uid_validity: UidValidity,
        uid_next: Option<ImapUid>,
        highest_modseq: Option<ModSeq>,
        exists: u32,
    ) -> Self {
        Self {
            uid_validity,
            uid_next,
            highest_modseq,
            exists,
        }
    }

    /// Returns the folder's `UIDVALIDITY`.
    #[must_use]
    pub const fn uid_validity(&self) -> UidValidity {
        self.uid_validity
    }

    /// Returns the next `UID`, if known.
    #[must_use]
    pub const fn uid_next(&self) -> Option<ImapUid> {
        self.uid_next
    }

    /// Returns the `HIGHESTMODSEQ`, if known.
    #[must_use]
    pub const fn highest_modseq(&self) -> Option<ModSeq> {
        self.highest_modseq
    }

    /// Returns how many messages the folder holds.
    #[must_use]
    pub const fn exists(&self) -> u32 {
        self.exists
    }
}

/// A label Gmail defines itself, written like a flag in `X-GM-LABELS` (`\Inbox`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SystemLabel {
    /// `\Inbox`.
    Inbox,
    /// `\Sent`.
    Sent,
    /// `\Draft`.
    Draft,
    /// `\Important`.
    Important,
    /// `\Starred`.
    Starred,
    /// `\Trash`.
    Trash,
    /// `\Spam`.
    Spam,
    /// Any other system label, by its name without the backslash. Not personal: Gmail defines
    /// these names.
    Other(String),
}

/// One label of a Gmail message.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GmailLabel {
    /// A label Gmail defines.
    System(SystemLabel),
    /// A user label, as the server named it: matched against the folder list by its raw name.
    Name(RemoteName),
}

/// What Gmail adds to a message: its ids and labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GmailItems {
    message_id: GmailMessageId,
    thread_id: GmailThreadId,
    labels: Vec<GmailLabel>,
}

impl GmailItems {
    /// Creates Gmail's items of a message.
    ///
    /// # Arguments
    ///
    /// * `message_id` - `X-GM-MSGID`.
    /// * `thread_id` - `X-GM-THRID`.
    /// * `labels` - `X-GM-LABELS`, in the server's order.
    #[must_use]
    pub const fn new(
        message_id: GmailMessageId,
        thread_id: GmailThreadId,
        labels: Vec<GmailLabel>,
    ) -> Self {
        Self {
            message_id,
            thread_id,
            labels,
        }
    }

    /// Returns the message id.
    #[must_use]
    pub const fn message_id(&self) -> GmailMessageId {
        self.message_id
    }

    /// Returns the thread id.
    #[must_use]
    pub const fn thread_id(&self) -> GmailThreadId {
        self.thread_id
    }

    /// Returns the labels.
    #[must_use]
    pub fn labels(&self) -> &[GmailLabel] {
        &self.labels
    }
}

/// One message's header as fetched from an opened folder. `Debug` hides personal data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteHeader {
    uid: ImapUid,
    flags: MessageFlags,
    header: MessageHeader,
    gmail: Option<GmailItems>,
}

impl RemoteHeader {
    /// Creates a fetched header.
    ///
    /// # Arguments
    ///
    /// * `uid` - The message's `UID` in the opened folder (on Gmail, `[Gmail]/All Mail`).
    /// * `flags` - Its flags and keywords.
    /// * `header` - Its parsed header, arrival time and size.
    /// * `gmail` - Gmail's ids and labels, on Gmail.
    #[must_use]
    pub const fn new(
        uid: ImapUid,
        flags: MessageFlags,
        header: MessageHeader,
        gmail: Option<GmailItems>,
    ) -> Self {
        Self {
            uid,
            flags,
            header,
            gmail,
        }
    }

    /// Returns the `UID`.
    #[must_use]
    pub const fn uid(&self) -> ImapUid {
        self.uid
    }

    /// Returns the flags.
    #[must_use]
    pub const fn flags(&self) -> &MessageFlags {
        &self.flags
    }

    /// Returns the header.
    #[must_use]
    pub const fn header(&self) -> &MessageHeader {
        &self.header
    }

    /// Returns Gmail's items, on Gmail.
    #[must_use]
    pub const fn gmail(&self) -> Option<&GmailItems> {
        self.gmail.as_ref()
    }
}

/// The headers of one fetch, and how many messages could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderBatch {
    headers: Vec<RemoteHeader>,
    skipped: usize,
}

impl HeaderBatch {
    /// Creates a batch.
    ///
    /// # Arguments
    ///
    /// * `headers` - The messages read, in the server's order.
    /// * `skipped` - Messages left out: a response that could not be parsed, or one without the
    ///   items asked for. Sync carries on without them.
    #[must_use]
    pub const fn new(headers: Vec<RemoteHeader>, skipped: usize) -> Self {
        Self { headers, skipped }
    }

    /// Returns the messages read.
    #[must_use]
    pub fn headers(&self) -> &[RemoteHeader] {
        &self.headers
    }

    /// Takes the messages read.
    #[must_use]
    pub fn into_headers(self) -> Vec<RemoteHeader> {
        self.headers
    }

    /// Returns how many messages were left out.
    #[must_use]
    pub const fn skipped(&self) -> usize {
        self.skipped
    }
}

/// How talking to a mail server failed, as far as the engine's rules care.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailErrorKind {
    /// The server refused the credentials: refresh the token, or sign in again.
    AuthenticationFailed,
    /// The server could not be reached or the connection broke: no connection, a TLS failure, a
    /// timeout.
    Network,
    /// The server is reachable but busy or temporarily refusing (too many connections, a
    /// bandwidth limit, maintenance). Retry later; the credentials are fine.
    Unavailable,
    /// The server said something the client cannot use, or lacks something it needs.
    Protocol,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gmail_listens_on_993() {
        let server = ImapServer::gmail();

        assert_eq!(
            (server.host(), server.port()),
            ("imap.gmail.com", 993),
            "Gmail"
        );
    }

    #[test]
    fn debug_output_hides_folder_names() {
        let mailbox = RemoteMailbox::new(
            RemoteName::new(b"Clients/Secret".to_vec()),
            MailboxPath::new("Clients/Secret", Some('/')).unwrap(),
            MailboxRole::Custom,
            true,
        );
        let listing = MailboxListing::new(vec![mailbox], 1);

        let output = format!("{listing:?}");

        assert!(!output.contains("Secret"), "leaked: {output}");
        assert!(output.contains("skipped: 1"), "count shown: {output}");
    }

    #[test]
    fn debug_output_hides_label_names() {
        let items = GmailItems::new(
            GmailMessageId::new(1),
            GmailThreadId::new(2),
            vec![
                GmailLabel::System(SystemLabel::Inbox),
                GmailLabel::Name(RemoteName::new(b"Clients/Secret".to_vec())),
            ],
        );

        let output = format!("{items:?}");

        assert!(!output.contains("Secret"), "leaked: {output}");
        assert!(
            output.contains("Inbox"),
            "system labels are not personal: {output}"
        );
    }
}
