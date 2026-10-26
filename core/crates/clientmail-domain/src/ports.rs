//! Traits the engine uses to reach storage and the network.
//!
//! Infrastructure crates implement them; the engine and the tests depend only on the traits.

use std::error::Error;

use crate::{
    Account, AccountId, BlobId, EmailAddress, Mailbox, MailboxId, MailboxSyncState, Message,
    MessageId, UnixTime,
};

/// Persists accounts.
///
/// Implementations decide the storage format. They hold no business rules: duplicate checks and
/// validation happen in the engine.
pub trait AccountRepository {
    /// The storage error type. `Send + Sync + 'static` lets the engine wrap it in its own error.
    type Error: Error + Send + Sync + 'static;

    /// Returns every account, oldest first.
    ///
    /// # Errors
    ///
    /// Returns the storage error when the read fails.
    fn list(&self) -> Result<Vec<Account>, Self::Error>;

    /// Returns the account with this address, if any.
    ///
    /// # Arguments
    ///
    /// * `address` - The normalised address to look up.
    ///
    /// # Errors
    ///
    /// Returns the storage error when the read fails.
    fn find_by_address(&self, address: &EmailAddress) -> Result<Option<Account>, Self::Error>;

    /// Stores a new account.
    ///
    /// # Arguments
    ///
    /// * `account` - The account to store.
    ///
    /// # Errors
    ///
    /// Returns the storage error when the write fails.
    fn insert(&self, account: &Account) -> Result<(), Self::Error>;

    /// Deletes an account.
    ///
    /// # Arguments
    ///
    /// * `id` - The account to delete.
    ///
    /// # Returns
    ///
    /// `true` when an account was deleted, `false` when none had this id.
    ///
    /// # Errors
    ///
    /// Returns the storage error when the write fails.
    fn delete(&self, id: AccountId) -> Result<bool, Self::Error>;
}

/// Persists the mailboxes of each account and where their sync resumes.
pub trait MailboxRepository {
    /// The storage error type.
    type Error: Error + Send + Sync + 'static;

    /// Returns the mailboxes of an account, ordered by path.
    ///
    /// # Arguments
    ///
    /// * `account` - The account to list.
    ///
    /// # Errors
    ///
    /// Returns the storage error when the read fails.
    fn mailboxes(&self, account: AccountId) -> Result<Vec<Mailbox>, Self::Error>;

    /// Stores a mailbox, or updates path, role and selectability of the one with the same id.
    ///
    /// Updating by id is what makes a server-side rename an update rather than a delete plus an
    /// insert.
    ///
    /// # Arguments
    ///
    /// * `mailbox` - The mailbox to store.
    ///
    /// # Errors
    ///
    /// Returns the storage error when the write fails, including when another mailbox of the same
    /// account already has this path.
    fn upsert_mailbox(&self, mailbox: &Mailbox) -> Result<(), Self::Error>;

    /// Deletes a mailbox, the plain IMAP messages it holds, and its Gmail labels.
    ///
    /// # Arguments
    ///
    /// * `id` - The mailbox to delete.
    ///
    /// # Returns
    ///
    /// `true` when a mailbox was deleted, `false` when none had this id.
    ///
    /// # Errors
    ///
    /// Returns the storage error when the write fails.
    fn delete_mailbox(&self, id: MailboxId) -> Result<bool, Self::Error>;

    /// Returns where sync of a mailbox resumes.
    ///
    /// # Arguments
    ///
    /// * `id` - The mailbox.
    ///
    /// # Returns
    ///
    /// `None` when the mailbox was never synced, or does not exist.
    ///
    /// # Errors
    ///
    /// Returns the storage error when the read fails.
    fn sync_state(&self, id: MailboxId) -> Result<Option<MailboxSyncState>, Self::Error>;

    /// Records where sync of a mailbox resumes.
    ///
    /// # Arguments
    ///
    /// * `id` - The mailbox.
    /// * `state` - The new sync state.
    ///
    /// # Errors
    ///
    /// Returns the storage error when the write fails or no mailbox has this id.
    fn set_sync_state(&self, id: MailboxId, state: &MailboxSyncState) -> Result<(), Self::Error>;
}

/// Where a page of a message list ends, so the next page starts right after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageCursor {
    /// `received_at` of the last message of the previous page.
    pub received_at: UnixTime,
    /// Id of the last message of the previous page; breaks ties between equal times.
    pub id: MessageId,
}

/// Which page of a mailbox's message list to read. Messages come newest first.
///
/// Fields are public because this is a plain parameter object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageQuery {
    /// The mailbox (plain IMAP folder or Gmail label) to list.
    pub mailbox: MailboxId,
    /// The largest number of messages to return.
    pub limit: u32,
    /// Where the previous page ended, or `None` for the first page.
    pub before: Option<PageCursor>,
}

/// Persists messages: identity, header, flags and Gmail labels.
pub trait MessageRepository {
    /// The storage error type.
    type Error: Error + Send + Sync + 'static;

    /// Inserts new messages and updates header and flags of known ones, in one transaction.
    ///
    /// A message is known when a stored one has the same server key: mailbox, `UIDVALIDITY` and
    /// `UID` for plain IMAP, or account and `X-GM-MSGID` for Gmail. A known message keeps its
    /// stored id.
    ///
    /// # Arguments
    ///
    /// * `messages` - The messages to store.
    ///
    /// # Returns
    ///
    /// The stored id of each message, in input order. Use these, not the ids the messages were
    /// built with: a known message keeps its original id.
    ///
    /// # Errors
    ///
    /// Returns the storage error when the write fails. Nothing is stored in that case.
    fn upsert_messages(&self, messages: &[Message]) -> Result<Vec<MessageId>, Self::Error>;

    /// Replaces the Gmail labels of a message.
    ///
    /// # Arguments
    ///
    /// * `id` - The message.
    /// * `labels` - The mailboxes that represent its labels, including All Mail.
    ///
    /// # Errors
    ///
    /// Returns the storage error when the write fails.
    fn set_labels(&self, id: MessageId, labels: &[MailboxId]) -> Result<(), Self::Error>;

    /// Returns one message.
    ///
    /// # Arguments
    ///
    /// * `id` - The message.
    ///
    /// # Errors
    ///
    /// Returns the storage error when the read fails.
    fn message(&self, id: MessageId) -> Result<Option<Message>, Self::Error>;

    /// Returns one page of a mailbox's message list, newest `received_at` first.
    ///
    /// # Arguments
    ///
    /// * `query` - The mailbox, page size and where the previous page ended.
    ///
    /// # Errors
    ///
    /// Returns the storage error when the read fails.
    fn page(&self, query: &PageQuery) -> Result<Vec<Message>, Self::Error>;
}

/// Stores message bodies and attachments by content.
pub trait BlobStore {
    /// The storage error type.
    type Error: Error + Send + Sync + 'static;

    /// Stores `bytes` and returns their id. Storing the same content twice keeps one copy.
    ///
    /// # Arguments
    ///
    /// * `bytes` - The content.
    ///
    /// # Errors
    ///
    /// Returns the storage error when the write fails.
    fn put(&self, bytes: &[u8]) -> Result<BlobId, Self::Error>;

    /// Returns the content of a blob.
    ///
    /// # Arguments
    ///
    /// * `id` - The blob.
    ///
    /// # Returns
    ///
    /// `None` when no blob has this id.
    ///
    /// # Errors
    ///
    /// Returns the storage error when the read fails or the content no longer matches its id.
    fn get(&self, id: BlobId) -> Result<Option<Vec<u8>>, Self::Error>;
}
