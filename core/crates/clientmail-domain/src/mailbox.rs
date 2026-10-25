use std::fmt;

use crate::id::uuid_id;
use crate::redact::Redacted;
use crate::{AccountId, DomainError};

uuid_id!(
    /// Identifies a mailbox (a folder, or a label on Gmail) on this Mac.
    MailboxId,
    DomainError::InvalidMailboxId
);

/// The name IMAP reserves for the inbox. It is case-insensitive (RFC 3501, section 5.1).
const INBOX: &str = "INBOX";

/// The server's name for a mailbox, already decoded to Unicode, and its hierarchy delimiter.
///
/// The name is not the mailbox's identity: folders get renamed, so storage keys mailboxes on
/// [`MailboxId`]. `Debug` hides the name, which can be personal (a client's name, a project).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct MailboxPath {
    name: String,
    delimiter: Option<char>,
}

impl MailboxPath {
    /// Validates a mailbox name.
    ///
    /// A first segment spelled `inbox` in any case is stored as `INBOX`, so every spelling the
    /// server uses compares equal.
    ///
    /// # Arguments
    ///
    /// * `name` - The full name as the server lists it, for example `Work/Clients`.
    /// * `delimiter` - The hierarchy separator the server reported, or `None` for a flat
    ///   namespace.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::InvalidMailboxPath`] when the name is empty, holds control
    /// characters, or has an empty segment (`a//b`, a leading or trailing delimiter). Callers that
    /// read a folder list from a server skip such a folder and keep the others.
    pub fn new(name: &str, delimiter: Option<char>) -> Result<Self, DomainError> {
        let invalid = |reason| DomainError::InvalidMailboxPath { reason };
        if name.is_empty() {
            return Err(invalid("the name is empty"));
        }
        if name.chars().any(char::is_control) {
            return Err(invalid("the name contains control characters"));
        }
        let path = Self {
            name: normalize_inbox(name, delimiter),
            delimiter,
        };
        if path.segments().any(str::is_empty) {
            return Err(invalid("the name has an empty segment"));
        }
        Ok(path)
    }

    /// Returns the full name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.name
    }

    /// Returns the hierarchy delimiter, if the server uses one.
    #[must_use]
    pub const fn delimiter(&self) -> Option<char> {
        self.delimiter
    }

    /// Returns the parts of the name from the top level down.
    ///
    /// The return type borrows from `self`: the segments are slices of the stored name, so no
    /// strings are copied.
    pub fn segments(&self) -> impl Iterator<Item = &str> {
        // Without a delimiter, splitting on a character the name cannot contain yields the whole
        // name as one segment, and both branches share one iterator type.
        self.name.split(self.delimiter.unwrap_or('\0'))
    }

    /// Returns the last segment: the name to show for this mailbox.
    #[must_use]
    pub fn leaf_name(&self) -> &str {
        self.split_last()
            .map_or(self.name.as_str(), |(_, leaf)| leaf)
    }

    /// Returns the path of the enclosing mailbox, or `None` at the top level.
    #[must_use]
    pub fn parent(&self) -> Option<Self> {
        self.split_last().map(|(parent, _)| Self {
            name: parent.to_owned(),
            delimiter: self.delimiter,
        })
    }

    fn split_last(&self) -> Option<(&str, &str)> {
        self.delimiter
            .and_then(|delimiter| self.name.rsplit_once(delimiter))
    }
}

impl fmt::Debug for MailboxPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MailboxPath")
            .field("name", &Redacted)
            .field("delimiter", &self.delimiter)
            .finish()
    }
}

/// Returns `name` with a first segment spelled `inbox` in any case rewritten as `INBOX`.
fn normalize_inbox(name: &str, delimiter: Option<char>) -> String {
    let first_end = delimiter
        .and_then(|delimiter| name.find(delimiter))
        .unwrap_or(name.len());
    // `split_at` cannot panic: `find` returns a character boundary, and `len` is one too.
    let (first, rest) = name.split_at(first_end);
    if first.eq_ignore_ascii_case(INBOX) {
        format!("{INBOX}{rest}")
    } else {
        name.to_owned()
    }
}

/// What a mailbox is for. System roles get their own icon, name and position in the sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MailboxRole {
    /// Incoming mail.
    Inbox,
    /// Every message of the account, whatever its folder (`\All`, Gmail's All Mail). Usually
    /// virtual: messages cannot be moved into it.
    AllMail,
    /// The folder archived messages move to (`\Archive`).
    Archive,
    /// Unsent messages.
    Drafts,
    /// Sent messages.
    Sent,
    /// Deleted messages.
    Trash,
    /// Spam.
    Junk,
    /// Starred or flagged messages.
    Starred,
    /// Gmail's priority marker.
    Important,
    /// A mailbox the user created.
    Custom,
}

/// A mailbox of an account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mailbox {
    id: MailboxId,
    account_id: AccountId,
    path: MailboxPath,
    role: MailboxRole,
    selectable: bool,
}

impl Mailbox {
    /// Creates a mailbox.
    ///
    /// # Arguments
    ///
    /// * `id` - The local id.
    /// * `account_id` - The account the mailbox belongs to.
    /// * `path` - The server name.
    /// * `role` - What the mailbox is for.
    /// * `selectable` - False for folders that only group other folders and hold no messages.
    #[must_use]
    pub const fn new(
        id: MailboxId,
        account_id: AccountId,
        path: MailboxPath,
        role: MailboxRole,
        selectable: bool,
    ) -> Self {
        Self {
            id,
            account_id,
            path,
            role,
            selectable,
        }
    }

    /// Returns the local id.
    #[must_use]
    pub const fn id(&self) -> MailboxId {
        self.id
    }

    /// Returns the account the mailbox belongs to.
    #[must_use]
    pub const fn account_id(&self) -> AccountId {
        self.account_id
    }

    /// Returns the server name.
    #[must_use]
    pub const fn path(&self) -> &MailboxPath {
        &self.path
    }

    /// Returns what the mailbox is for.
    #[must_use]
    pub const fn role(&self) -> MailboxRole {
        self.role
    }

    /// Returns true when the mailbox can hold messages.
    #[must_use]
    pub const fn is_selectable(&self) -> bool {
        self.selectable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_and_leaf_follow_the_delimiter() {
        let slash = MailboxPath::new("Work/Clients/Acme", Some('/')).unwrap();
        let dot = MailboxPath::new("INBOX.Receipts", Some('.')).unwrap();

        assert_eq!(
            slash.segments().collect::<Vec<_>>(),
            ["Work", "Clients", "Acme"],
            "slash segments"
        );
        assert_eq!(slash.leaf_name(), "Acme", "slash leaf");
        assert_eq!(dot.leaf_name(), "Receipts", "dot leaf");
    }

    #[test]
    fn flat_namespace_has_one_segment_and_no_parent() {
        let path = MailboxPath::new("Work/Clients", None).unwrap();

        assert_eq!(
            path.segments().collect::<Vec<_>>(),
            ["Work/Clients"],
            "slash is a plain character without a delimiter"
        );
        assert_eq!(path.leaf_name(), "Work/Clients", "leaf is the whole name");
        assert_eq!(path.parent(), None, "no hierarchy, no parent");
    }

    #[test]
    fn parent_walks_up_to_the_top_level() {
        let path = MailboxPath::new("[Gmail]/Sent Mail", Some('/')).unwrap();

        let parent = path.parent().unwrap();

        assert_eq!(parent.as_str(), "[Gmail]", "parent name");
        assert_eq!(parent.delimiter(), Some('/'), "parent keeps the delimiter");
        assert_eq!(parent.parent(), None, "top level has no parent");
    }

    #[test]
    fn inbox_is_case_insensitive() {
        let upper = MailboxPath::new("INBOX", Some('/')).unwrap();
        let mixed = MailboxPath::new("Inbox", Some('/')).unwrap();
        let child = MailboxPath::new("inbox/Receipts", Some('/')).unwrap();
        let flat_child = MailboxPath::new("inbox.Receipts", None).unwrap();

        assert_eq!(mixed, upper, "any spelling of INBOX is the same mailbox");
        assert_eq!(child.as_str(), "INBOX/Receipts", "child of the inbox");
        assert_eq!(
            flat_child.as_str(),
            "inbox.Receipts",
            "without a delimiter the whole name is one segment"
        );
    }

    #[test]
    fn inbox_lookalikes_are_not_rewritten() {
        let path = MailboxPath::new("Inboxes/Old", Some('/')).unwrap();

        assert_eq!(path.as_str(), "Inboxes/Old", "only an exact INBOX segment");
    }

    #[test]
    fn keeps_unicode_names() {
        let path = MailboxPath::new("Lavoro/Fatture è più", Some('/')).unwrap();

        assert_eq!(path.leaf_name(), "Fatture è più", "unicode leaf");
    }

    #[test]
    fn rejects_invalid_names() {
        for (name, delimiter) in [
            ("", Some('/')),
            ("Work\nClients", Some('/')),
            ("Work//Clients", Some('/')),
            ("/Work", Some('/')),
            ("Work/", Some('/')),
        ] {
            let result = MailboxPath::new(name, delimiter);

            assert!(
                matches!(result, Err(DomainError::InvalidMailboxPath { .. })),
                "{name:?} must be rejected, got {result:?}"
            );
        }
    }

    #[test]
    fn mailbox_exposes_its_parts() {
        let id = MailboxId::generate();
        let account_id = AccountId::generate();
        let path = MailboxPath::new("INBOX", Some('/')).unwrap();

        let mailbox = Mailbox::new(id, account_id, path.clone(), MailboxRole::Inbox, true);

        assert_eq!(mailbox.id(), id, "id");
        assert_eq!(mailbox.account_id(), account_id, "account");
        assert_eq!(mailbox.path(), &path, "path");
        assert_eq!(mailbox.role(), MailboxRole::Inbox, "role");
        assert!(mailbox.is_selectable(), "selectable");
    }
}
