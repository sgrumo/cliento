//! Traits the engine uses to reach storage and the network.
//!
//! Infrastructure crates implement them; the engine and the tests depend only on the traits.

use std::error::Error;

use secrecy::SecretString;

use crate::{
    Account, AccountId, AuthorizationCode, AuthorizationRequest, BlobId, EmailAddress, HeaderBatch,
    ImapServer, ImapUid, MailErrorKind, Mailbox, MailboxId, MailboxListing, MailboxSyncState,
    Message, MessageId, OAuthErrorKind, OAuthState, PkceVerifier, RemoteName, SecretKey,
    SecretKind, SelectedMailbox, TokenGrant, UnixTime,
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

    /// Returns whether an account with this id is stored.
    ///
    /// # Arguments
    ///
    /// * `id` - The account to look for.
    ///
    /// # Errors
    ///
    /// Returns the storage error when the read fails.
    fn exists(&self, id: AccountId) -> Result<bool, Self::Error>;

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

/// Stores account secrets (OAuth tokens, passwords) outside the database.
///
/// On macOS the app implements it with the Keychain. Implementations hold no rules: when to read,
/// write or delete is decided by the engine.
pub trait SecretStore {
    /// The storage error type. Its messages must never include a secret.
    type Error: Error + Send + Sync + 'static;

    /// Returns a secret.
    ///
    /// # Arguments
    ///
    /// * `key` - Which secret.
    ///
    /// # Returns
    ///
    /// `None` when no secret is stored under `key`.
    ///
    /// # Errors
    ///
    /// Returns the storage error when the read fails.
    fn read(&self, key: &SecretKey) -> Result<Option<SecretString>, Self::Error>;

    /// Stores a secret, replacing any previous value under `key`.
    ///
    /// # Arguments
    ///
    /// * `key` - Which secret.
    /// * `secret` - The value.
    ///
    /// # Errors
    ///
    /// Returns the storage error when the write fails.
    fn write(&self, key: &SecretKey, secret: &SecretString) -> Result<(), Self::Error>;

    /// Deletes a secret.
    ///
    /// # Arguments
    ///
    /// * `key` - Which secret.
    ///
    /// # Returns
    ///
    /// `true` when a secret was deleted, `false` when none was stored.
    ///
    /// # Errors
    ///
    /// Returns the storage error when the delete fails.
    fn delete(&self, key: &SecretKey) -> Result<bool, Self::Error>;

    /// Deletes every secret of an account. Kinds with nothing stored are skipped.
    ///
    /// # Arguments
    ///
    /// * `account` - The account whose secrets to delete.
    ///
    /// # Errors
    ///
    /// Returns the storage error of the first delete that fails. Kinds deleted before it stay
    /// deleted, so a failure can leave some of the account's secrets in place and others gone.
    fn delete_account(&self, account: AccountId) -> Result<(), Self::Error> {
        SecretKind::ALL
            .into_iter()
            .try_for_each(|kind| self.delete(&SecretKey::new(account, kind)).map(|_| ()))
    }
}

/// One OAuth provider's protocol: authorization URL, callback, token endpoint.
///
/// Implementations hold protocol details only; which account a grant belongs to, and what is
/// stored, is decided in the engine.
///
/// The async methods return `impl Future + Send` instead of being `async fn`, because an
/// `async fn` in a public trait cannot promise a `Send` future, which a multi-thread runtime
/// needs.
pub trait OAuthClient: Send + Sync + 'static {
    /// The protocol error type.
    type Error: Error + Send + Sync + 'static;

    /// Classifies an error for the engine's rules.
    ///
    /// # Arguments
    ///
    /// * `error` - An error this client returned.
    fn error_kind(error: &Self::Error) -> OAuthErrorKind;

    /// Starts an authorization: a fresh PKCE verifier and state, and the URL to open.
    ///
    /// # Errors
    ///
    /// Returns the protocol error when the OS cannot provide random bytes.
    fn authorization(&self) -> Result<AuthorizationRequest, Self::Error>;

    /// Returns the URL scheme the browser sheet waits for.
    fn callback_scheme(&self) -> &str;

    /// Reads the authorization code out of the callback URL, after checking it belongs to this
    /// authorization.
    ///
    /// The state is checked before anything else, error callbacks included, so a callback of
    /// another sign-in can neither complete nor end this one (RFC 6749, section 10.12).
    ///
    /// # Arguments
    ///
    /// * `url` - The URL the browser was redirected to.
    /// * `expected` - The state this authorization sent.
    ///
    /// # Errors
    ///
    /// Returns an error of kind [`OAuthErrorKind::Denied`] when the user refused consent, and of
    /// kind [`OAuthErrorKind::Protocol`] when the URL is not a callback of this authorization.
    fn parse_callback(
        &self,
        url: &str,
        expected: &OAuthState,
    ) -> Result<AuthorizationCode, Self::Error>;

    /// Exchanges an authorization code for tokens.
    ///
    /// # Arguments
    ///
    /// * `code` - The code from the callback.
    /// * `verifier` - The verifier of the same authorization.
    ///
    /// # Errors
    ///
    /// Returns the protocol error, classified by [`OAuthClient::error_kind`].
    fn exchange(
        &self,
        code: AuthorizationCode,
        verifier: PkceVerifier,
    ) -> impl Future<Output = Result<TokenGrant, Self::Error>> + Send;

    /// Obtains a new access token with a refresh token.
    ///
    /// # Arguments
    ///
    /// * `refresh_token` - The stored refresh token.
    ///
    /// # Errors
    ///
    /// Returns the protocol error; [`OAuthErrorKind::Rejected`] means the user must sign in again.
    fn refresh(
        &self,
        refresh_token: SecretString,
    ) -> impl Future<Output = Result<TokenGrant, Self::Error>> + Send;
}

/// Runs synchronous work (storage, platform callbacks) where it cannot block async tasks.
///
/// Async use cases wrap every call to a synchronous port in [`Blocking::run`] (decision record
/// 0009).
pub trait Blocking: Send + Sync + 'static {
    /// Runs `work` and returns a future of its result.
    ///
    /// # Arguments
    ///
    /// * `work` - The synchronous work. It must own what it uses (`'static`), because it may run
    ///   on another thread.
    fn run<T, F>(&self, work: F) -> impl Future<Output = T> + Send + 'static
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static;
}

/// Tells the time, so tests can control it.
pub trait Clock: Send + Sync + 'static {
    /// Returns the current time.
    fn now(&self) -> UnixTime;
}

/// Opens authenticated sessions with an IMAP server. Implemented by `clientmail-imap`.
///
/// It speaks IMAP's vocabulary (folders, sessions); Microsoft Graph gets its own port.
pub trait MailConnector: Send + Sync + 'static {
    /// The protocol error type, shared with the sessions.
    type Error: Error + Send + Sync + 'static;
    /// An open, authenticated session. `Send + 'static` so a task can own it.
    type Session: MailSession<Error = Self::Error> + Send + 'static;

    /// Classifies an error for the engine's rules.
    ///
    /// # Arguments
    ///
    /// * `error` - An error this connector or one of its sessions returned.
    fn error_kind(error: &Self::Error) -> MailErrorKind;

    /// Connects over TLS, checks the server offers XOAUTH2, and signs in with an access token.
    ///
    /// # Arguments
    ///
    /// * `server` - Where the server listens.
    /// * `user` - The account's address.
    /// * `access_token` - A valid OAuth access token for the account.
    ///
    /// # Errors
    ///
    /// Returns an error of kind [`MailErrorKind::AuthenticationFailed`] when the server refuses
    /// the token, [`MailErrorKind::Unavailable`] when it is busy, [`MailErrorKind::Network`]
    /// when it cannot be reached, and [`MailErrorKind::Protocol`] when it does not offer XOAUTH2
    /// or answers unexpectedly.
    fn connect(
        &self,
        server: &ImapServer,
        user: &EmailAddress,
        access_token: &SecretString,
    ) -> impl Future<Output = Result<Self::Session, Self::Error>> + Send;
}

/// An authenticated IMAP session.
///
/// After any error, a timeout included, the session is broken: the server may still be sending
/// the failed command's answer, which the next command would misread. Every later call fails with
/// a [`MailErrorKind::Network`] error; drop the session and connect again. Dropping a call's
/// future part-way (cancelling it) leaves the session in the same state.
pub trait MailSession: Send {
    /// The protocol error type.
    type Error: Error + Send + Sync + 'static;

    /// Lists every folder of the account with its role.
    ///
    /// Folders whose names cannot be used are left out and counted, so the rest still sync.
    ///
    /// # Errors
    ///
    /// Returns the protocol error when the listing fails.
    fn list_mailboxes(
        &mut self,
    ) -> impl Future<Output = Result<MailboxListing, Self::Error>> + Send;

    /// Returns whether the server speaks Gmail's IMAP extensions (`X-GM-EXT-1`): message and
    /// thread ids and labels.
    fn has_gmail_extensions(&self) -> bool;

    /// Opens a folder read-only (`EXAMINE`), so reading never marks mail as read.
    ///
    /// # Arguments
    ///
    /// * `mailbox` - The folder's name as the server listed it.
    ///
    /// # Errors
    ///
    /// Returns the protocol error when the folder cannot be opened or its `UIDVALIDITY` is
    /// missing.
    fn examine(
        &mut self,
        mailbox: &RemoteName,
    ) -> impl Future<Output = Result<SelectedMailbox, Self::Error>> + Send;

    /// Returns the `UID`s of the opened folder's messages that arrived at or after `since`, in
    /// ascending order.
    ///
    /// # Arguments
    ///
    /// * `since` - The earliest arrival time wanted. IMAP searches whole days in the server's
    ///   time zone, so messages from up to two days earlier can come back too; none after `since`
    ///   is missed.
    ///
    /// # Errors
    ///
    /// Returns the protocol error when the search fails or no folder is open.
    fn search_since(
        &mut self,
        since: UnixTime,
    ) -> impl Future<Output = Result<Vec<ImapUid>, Self::Error>> + Send;

    /// Fetches the headers, flags, arrival time and size of messages in the opened folder, and on
    /// Gmail their ids and labels.
    ///
    /// # Arguments
    ///
    /// * `uids` - The messages to fetch, sent as one command. Keep it to a few hundred and page
    ///   from the newest, so the first page shows quickly and a failure loses one page only.
    ///
    /// # Errors
    ///
    /// Returns the protocol error when the fetch fails. A `UID` asked for that does not come back
    /// complete (unknown, expunged, unreadable) is counted in [`HeaderBatch::skipped`], not an
    /// error.
    fn fetch_headers(
        &mut self,
        uids: &[ImapUid],
    ) -> impl Future<Output = Result<HeaderBatch, Self::Error>> + Send;

    /// Ends the session politely.
    ///
    /// # Errors
    ///
    /// Returns the protocol error when the server cannot be told; the connection closes anyway.
    fn logout(self) -> impl Future<Output = Result<(), Self::Error>> + Send;
}
