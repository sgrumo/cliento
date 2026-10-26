use std::borrow::Cow;
use std::collections::HashSet;
use std::time::Duration;

use clientmail_domain::ports::MailSession;
use clientmail_domain::{
    EmailAddress, ExposeSecret, HeaderBatch, ImapUid, MailboxListing, MailboxPath, ModSeq,
    RemoteMailbox, RemoteName, SecretString, SelectedMailbox, UidValidity, UnixTime,
};
use imap_next::client::{Client, Error as ClientError, Event, Options};
use imap_next::imap_types::IntoStatic as _;
use imap_next::imap_types::auth::{AuthMechanism, AuthenticateData};
use imap_next::imap_types::command::{Command, CommandBody};
use imap_next::imap_types::core::{AString, IString, Tag, Vec1};
use imap_next::imap_types::datetime::NaiveDate;
use imap_next::imap_types::fetch::MacroOrMessageDataItemNames;
use imap_next::imap_types::flag::FlagNameAttribute;
use imap_next::imap_types::mailbox::{ListMailbox, Mailbox};
use imap_next::imap_types::response::{
    Capability, Code, Data, Greeting, GreetingKind, Status, StatusKind,
};
use imap_next::imap_types::search::SearchKey;
use imap_next::imap_types::secret::Secret;
use imap_next::imap_types::sequence::SequenceSet;
use imap_next::{Interrupt, Io, State as _};
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};

use crate::{ImapError, fetch, roles, utf7};

/// How long the server may stay silent while a command runs. Each response resets it, so a long
/// listing on a slow link is fine as long as data keeps coming.
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);
/// The longest sign-in may take, from the greeting to the end of `AUTHENTICATE`.
const OPEN_TIMEOUT: Duration = Duration::from_secs(60);
/// How many bytes one read takes from the stream.
const READ_CHUNK: usize = 16 * 1024;

/// An authenticated IMAP session over any stream.
///
/// The app uses a TLS stream; tests use an in-memory pipe and a scripted server. The protocol
/// state lives in `imap-next`'s sans-I/O client; this type moves the bytes and sends one command
/// at a time.
pub struct ImapSession<S> {
    stream: S,
    client: Client,
    last_tag: u32,
    buffer: Vec<u8>,
    /// Set while a command runs and cleared when its tagged response arrives. Still set at the
    /// start of a call means the previous one failed, timed out or was cancelled part-way.
    busy: bool,
    /// The server's capabilities: those sent after sign-in when it sent any, else those from
    /// before (Gmail lists `X-GM-EXT-1` in both).
    capabilities: Vec<Capability<'static>>,
}

impl<S> ImapSession<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    /// Reads the greeting, checks the server offers XOAUTH2, and signs in.
    ///
    /// # Arguments
    ///
    /// * `stream` - A connected stream, encrypted already.
    /// * `user` - The account's address.
    /// * `access_token` - The OAuth access token.
    ///
    /// # Errors
    ///
    /// Returns [`ImapError::NoXoauth2`] before sending the token when the server does not offer
    /// it, [`ImapError::AuthenticationFailed`] when it refuses the token,
    /// [`ImapError::Unavailable`] when it is busy, [`ImapError::Timeout`] after 60 s in all,
    /// [`ImapError::Network`] or [`ImapError::Protocol`].
    pub async fn open(
        stream: S,
        user: &EmailAddress,
        access_token: &SecretString,
    ) -> Result<Self, ImapError> {
        let mut session = Self {
            stream,
            client: Client::new(Options::default()),
            last_tag: 0,
            buffer: vec![0; READ_CHUNK],
            busy: false,
            capabilities: Vec::new(),
        };
        tokio::time::timeout(OPEN_TIMEOUT, session.sign_in(user, access_token))
            .await
            .map_err(|_| ImapError::Timeout)??;
        Ok(session)
    }

    async fn sign_in(
        &mut self,
        user: &EmailAddress,
        access_token: &SecretString,
    ) -> Result<(), ImapError> {
        let greeting = self.greeting().await?;
        let capabilities = match greeting_capabilities(&greeting)? {
            Some(capabilities) => capabilities,
            None => self.capabilities().await?,
        };
        if !capabilities.contains(&Capability::Auth(AuthMechanism::XOAuth2)) {
            return Err(ImapError::NoXoauth2);
        }
        let sasl_ir = capabilities.contains(&Capability::SaslIr);
        self.capabilities = capabilities;
        self.authenticate(user, access_token, sasl_ir).await
    }

    async fn greeting(&mut self) -> Result<Greeting<'static>, ImapError> {
        loop {
            if let Event::GreetingReceived { greeting } = self.next_event().await? {
                return Ok(greeting);
            }
        }
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "every other event is unrelated to CAPABILITY and skipped"
    )]
    async fn capabilities(&mut self) -> Result<Vec<Capability<'static>>, ImapError> {
        let tag = self.send(CommandBody::Capability)?;
        let mut capabilities = Vec::new();
        loop {
            match self.next_event().await? {
                Event::DataReceived {
                    data: Data::Capability(list),
                } => capabilities.extend(list),
                Event::StatusReceived {
                    status: Status::Tagged(tagged),
                } if tagged.tag == tag => {
                    self.busy = false;
                    return match tagged.body.kind {
                        StatusKind::Ok => Ok(capabilities),
                        StatusKind::No | StatusKind::Bad => {
                            Err(ImapError::Protocol("the server refused CAPABILITY"))
                        }
                    };
                }
                Event::StatusReceived {
                    status: Status::Bye(_),
                } => return Err(ImapError::Unavailable),
                _ => {}
            }
        }
    }

    /// Signs in with XOAUTH2, as Google documents it
    /// (<https://developers.google.com/gmail/imap/xoauth2-protocol>).
    ///
    /// The credentials go on the `AUTHENTICATE` line when the server offers SASL-IR, otherwise
    /// after its first `+`. When the server refuses the token it first sends a challenge (base64
    /// JSON describing the error), which Google's protocol answers with an empty line; only then
    /// does the server say `NO`.
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "every other event is unrelated to AUTHENTICATE and skipped"
    )]
    async fn authenticate(
        &mut self,
        user: &EmailAddress,
        access_token: &SecretString,
        sasl_ir: bool,
    ) -> Result<(), ImapError> {
        // Built in a `SecretString`, which zeroes this copy when dropped; the copies the IMAP
        // library makes while encoding cannot be zeroed (as for the Keychain bridge, 0007).
        let credentials = SecretString::from(format!(
            "user={}\u{1}auth=Bearer {}\u{1}\u{1}",
            user.as_str(),
            access_token.expose_secret()
        ));
        let sasl = || Secret::new(Cow::Owned(credentials.expose_secret().as_bytes().to_vec()));
        let mut credentials_sent = sasl_ir;
        self.send(CommandBody::Authenticate {
            mechanism: AuthMechanism::XOAuth2,
            initial_response: sasl_ir.then(sasl),
        })?;
        loop {
            match self.next_event().await? {
                Event::AuthenticateContinuationRequestReceived { .. } => {
                    let answer = if credentials_sent {
                        Secret::new(Cow::Owned(Vec::new()))
                    } else {
                        credentials_sent = true;
                        sasl()
                    };
                    self.client
                        .set_authenticate_data(AuthenticateData::Continue(answer))
                        .map_err(|_| {
                            ImapError::Protocol("the client could not answer the server")
                        })?;
                }
                Event::DataReceived {
                    data: Data::Capability(list),
                } => self.capabilities = list.into_iter().collect(),
                Event::AuthenticateStatusReceived { status, .. } => {
                    self.busy = false;
                    return match status {
                        Status::Tagged(tagged) => match tagged.body.kind {
                            StatusKind::Ok => {
                                if let Some(Code::Capability(list)) = tagged.body.code {
                                    self.capabilities = list.into_iter().collect();
                                }
                                Ok(())
                            }
                            StatusKind::No | StatusKind::Bad => {
                                Err(classify_refusal(tagged.body.code.as_ref()))
                            }
                        },
                        Status::Bye(_) => Err(ImapError::Unavailable),
                        Status::Untagged(_) => Err(ImapError::Protocol("AUTHENTICATE ended oddly")),
                    };
                }
                _ => {}
            }
        }
    }

    /// Queues a command with the next tag (`A0001`, `A0002`, ...) and marks the session busy.
    fn send(&mut self, body: CommandBody<'static>) -> Result<Tag<'static>, ImapError> {
        if self.busy {
            return Err(ImapError::Broken);
        }
        self.last_tag += 1;
        let tag = Tag::try_from(format!("A{:04}", self.last_tag))
            .map_err(|_| ImapError::Protocol("a command tag could not be built"))?;
        self.client.enqueue_command(Command {
            tag: tag.clone(),
            body,
        });
        self.busy = true;
        Ok(tag)
    }

    /// Moves bytes until the client produces the next event.
    ///
    /// # Errors
    ///
    /// Returns [`ImapError::Unparsable`] for a response the codec could not parse (it is
    /// discarded and the connection stays usable), [`ImapError::Timeout`] after 60 s of silence,
    /// [`ImapError::Network`] for I/O errors and end of file, and [`ImapError::Protocol`] for a
    /// response too long to accept.
    async fn next_event(&mut self) -> Result<Event, ImapError> {
        loop {
            match self.client.next() {
                Ok(event) => return Ok(event),
                Err(Interrupt::Io(Io::Output(bytes))) => {
                    tokio::time::timeout(IDLE_TIMEOUT, async {
                        self.stream.write_all(&bytes).await?;
                        self.stream.flush().await
                    })
                    .await
                    .map_err(|_| ImapError::Timeout)?
                    .map_err(|_| ImapError::Network)?;
                }
                Err(Interrupt::Io(Io::NeedMoreInput)) => {
                    let read =
                        tokio::time::timeout(IDLE_TIMEOUT, self.stream.read(&mut self.buffer))
                            .await
                            .map_err(|_| ImapError::Timeout)?
                            .map_err(|_| ImapError::Network)?;
                    let bytes = self.buffer.get(..read).ok_or(ImapError::Network)?;
                    if bytes.is_empty() {
                        return Err(ImapError::Network);
                    }
                    self.client.enqueue_input(bytes);
                }
                Err(Interrupt::Error(ClientError::MalformedMessage { .. })) => {
                    return Err(ImapError::Unparsable);
                }
                Err(Interrupt::Error(
                    ClientError::ResponseTooLong { .. } | ClientError::ExpectedCrlfGotLf { .. },
                )) => return Err(ImapError::Protocol("the server's answer could not be read")),
            }
        }
    }

    /// Sends one `UID FETCH` for `uids` and reads it to its tagged completion.
    ///
    /// A message's items can arrive in several `FETCH` responses (and the server can push flag
    /// changes the same way), so items are merged per sequence number and turned into headers
    /// only once the command is done; messages not asked for are ignored.
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "every other event is unrelated to FETCH and ignored"
    )]
    async fn fetch(&mut self, uids: &[ImapUid], gmail: bool) -> Result<HeaderBatch, ImapError> {
        let sequence_set = SequenceSet::try_from(fetch::sequence_set(uids).as_str())
            .map_err(|_| ImapError::Protocol("FETCH could not be built"))?;
        let tag = self.send(CommandBody::Fetch {
            sequence_set,
            macro_or_item_names: MacroOrMessageDataItemNames::MessageDataItemNames(
                fetch::item_names(gmail),
            ),
            uid: true,
            modifiers: Vec::new(),
        })?;
        let mut responses = fetch::Responses::default();
        loop {
            let event = match self.next_event().await {
                // One line the codec cannot read: its message is reported missing below.
                Err(ImapError::Unparsable) => continue,
                event => event?,
            };
            match event {
                Event::DataReceived {
                    data: Data::Fetch { seq, items },
                } => responses.add(seq, items.into_inner()),
                Event::StatusReceived {
                    status: Status::Tagged(tagged),
                } if tagged.tag == tag => {
                    self.busy = false;
                    return match tagged.body.kind {
                        StatusKind::Ok => Ok(responses.into_batch(uids, gmail)),
                        StatusKind::No | StatusKind::Bad => Err(classify_command_refusal(
                            tagged.body.code.as_ref(),
                            "the server refused FETCH",
                        )),
                    };
                }
                Event::StatusReceived {
                    status: Status::Bye(_),
                } => return Err(ImapError::Network),
                _ => {}
            }
        }
    }
}

impl<S> MailSession for ImapSession<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    type Error = ImapError;

    /// Sends `LIST "" *` and reads until its own tagged completion; only `OK` counts.
    ///
    /// A line that cannot be parsed, a name that cannot be used, and a name that decodes to the
    /// path of a folder listed before it are skipped and counted.
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "every other event is unrelated to LIST and ignored"
    )]
    async fn list_mailboxes(&mut self) -> Result<MailboxListing, ImapError> {
        let tag = self.send(CommandBody::List {
            reference: Mailbox::try_from("")
                .map_err(|_| ImapError::Protocol("LIST could not be built"))?,
            mailbox_wildcard: ListMailbox::try_from("*")
                .map_err(|_| ImapError::Protocol("LIST could not be built"))?,
        })?;
        let mut mailboxes = Vec::new();
        let mut paths = HashSet::new();
        let mut skipped = 0;
        loop {
            let event = match self.next_event().await {
                Err(ImapError::Unparsable) => {
                    skipped += 1;
                    continue;
                }
                event => event?,
            };
            match event {
                Event::DataReceived {
                    data:
                        Data::List {
                            items,
                            delimiter,
                            mailbox,
                        },
                } => match remote_mailbox(
                    &items,
                    delimiter.map(|delimiter| delimiter.inner()),
                    &mailbox,
                ) {
                    Some(found) if paths.insert(found.path().clone()) => mailboxes.push(found),
                    Some(_) | None => skipped += 1,
                },
                Event::StatusReceived {
                    status: Status::Tagged(tagged),
                } if tagged.tag == tag => {
                    self.busy = false;
                    return match tagged.body.kind {
                        StatusKind::Ok => Ok(MailboxListing::new(mailboxes, skipped)),
                        StatusKind::No | StatusKind::Bad => Err(classify_command_refusal(
                            tagged.body.code.as_ref(),
                            "the server refused LIST",
                        )),
                    };
                }
                Event::StatusReceived {
                    status: Status::Bye(_),
                } => return Err(ImapError::Network),
                _ => {}
            }
        }
    }

    fn has_gmail_extensions(&self) -> bool {
        self.capabilities
            .iter()
            .any(|capability| capability.to_string().eq_ignore_ascii_case("X-GM-EXT-1"))
    }

    /// Sends `EXAMINE` with the folder's raw name and reads its `UIDVALIDITY`, `UIDNEXT`,
    /// `HIGHESTMODSEQ` and `EXISTS`.
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "every other event and response code is unrelated to EXAMINE and ignored"
    )]
    async fn examine(&mut self, mailbox: &RemoteName) -> Result<SelectedMailbox, ImapError> {
        let mailbox = Mailbox::try_from(mailbox.as_bytes().to_vec())
            .map_err(|_| ImapError::Protocol("the folder name cannot be sent"))?;
        let tag = self.send(CommandBody::Examine {
            mailbox,
            parameters: Vec::new(),
        })?;
        let (mut uid_validity, mut uid_next, mut highest_modseq, mut exists) =
            (None, None, None, 0);
        loop {
            let event = match self.next_event().await {
                Err(ImapError::Unparsable) => continue,
                event => event?,
            };
            match event {
                Event::DataReceived {
                    data: Data::Exists(count),
                } => exists = count,
                Event::StatusReceived {
                    status: Status::Untagged(body),
                } => match body.code {
                    Some(Code::UidValidity(value)) => uid_validity = UidValidity::new(value.get()),
                    Some(Code::UidNext(value)) => uid_next = ImapUid::new(value.get()),
                    Some(Code::HighestModSeq(value)) => highest_modseq = ModSeq::new(value.get()),
                    _ => {}
                },
                Event::StatusReceived {
                    status: Status::Tagged(tagged),
                } if tagged.tag == tag => {
                    self.busy = false;
                    return match tagged.body.kind {
                        StatusKind::Ok => uid_validity
                            .map(|validity| {
                                SelectedMailbox::new(validity, uid_next, highest_modseq, exists)
                            })
                            .ok_or(ImapError::Protocol("the folder has no UIDVALIDITY")),
                        StatusKind::No | StatusKind::Bad => Err(classify_command_refusal(
                            tagged.body.code.as_ref(),
                            "the server refused EXAMINE",
                        )),
                    };
                }
                Event::StatusReceived {
                    status: Status::Bye(_),
                } => return Err(ImapError::Network),
                _ => {}
            }
        }
    }

    /// Sends `UID SEARCH SINCE <day>`, the day before `since`'s UTC day (see `fetch::search_day`).
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "every other event is unrelated to SEARCH and ignored"
    )]
    async fn search_since(&mut self, since: UnixTime) -> Result<Vec<ImapUid>, ImapError> {
        let day = fetch::search_day(since)
            .and_then(|day| NaiveDate::try_from(day).ok())
            .ok_or(ImapError::Protocol("the date is out of range"))?;
        let tag = self.send(CommandBody::Search {
            charset: None,
            criteria: Vec1::from(SearchKey::Since(day)),
            uid: true,
        })?;
        let mut uids = Vec::new();
        loop {
            let event = match self.next_event().await {
                Err(ImapError::Unparsable) => continue,
                event => event?,
            };
            match event {
                Event::DataReceived {
                    data: Data::Search(found, ..),
                } => uids.extend(found.iter().filter_map(|uid| ImapUid::new(uid.get()))),
                Event::StatusReceived {
                    status: Status::Tagged(tagged),
                } if tagged.tag == tag => {
                    self.busy = false;
                    return match tagged.body.kind {
                        StatusKind::Ok => {
                            uids.sort_unstable();
                            uids.dedup();
                            Ok(uids)
                        }
                        StatusKind::No | StatusKind::Bad => Err(classify_command_refusal(
                            tagged.body.code.as_ref(),
                            "the server refused SEARCH",
                        )),
                    };
                }
                Event::StatusReceived {
                    status: Status::Bye(_),
                } => return Err(ImapError::Network),
                _ => {}
            }
        }
    }

    /// Sends one `UID FETCH` for `uids`, with Gmail's items when the server has them; nothing
    /// for an empty list.
    async fn fetch_headers(&mut self, uids: &[ImapUid]) -> Result<HeaderBatch, ImapError> {
        if uids.is_empty() {
            return Ok(HeaderBatch::new(Vec::new(), 0));
        }
        let gmail = self.has_gmail_extensions();
        self.fetch(uids, gmail).await
    }

    /// Sends `LOGOUT`. The server's `BYE` and closing the connection both count as done.
    async fn logout(mut self) -> Result<(), ImapError> {
        let tag = self.send(CommandBody::Logout)?;
        loop {
            match self.next_event().await {
                Ok(Event::StatusReceived {
                    status: Status::Tagged(tagged),
                }) if tagged.tag == tag => return Ok(()),
                Ok(_) | Err(ImapError::Unparsable) => {}
                Err(ImapError::Network) => return Ok(()),
                Err(error) => return Err(error),
            }
        }
    }
}

impl<S> std::fmt::Debug for ImapSession<S> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ImapSession")
    }
}

/// Reads the capabilities from the greeting's `CAPABILITY` code, or `None` when it has none.
fn greeting_capabilities(
    greeting: &Greeting<'_>,
) -> Result<Option<Vec<Capability<'static>>>, ImapError> {
    match greeting.kind {
        GreetingKind::Ok => Ok(match &greeting.code {
            Some(Code::Capability(list)) => Some(
                list.as_ref()
                    .iter()
                    .map(|capability| capability.clone().into_static())
                    .collect(),
            ),
            Some(_) | None => None,
        }),
        // A `BYE` greeting is how servers say "too busy, come back later" (RFC 3501, 7.1.5).
        GreetingKind::Bye => Err(ImapError::Unavailable),
        GreetingKind::PreAuth => Err(ImapError::Protocol(
            "the server signed in on its own (PREAUTH)",
        )),
    }
}

/// Returns the response code's name when it is one `imap-types` does not model (RFC 5530 codes
/// such as `AUTHENTICATIONFAILED` and `UNAVAILABLE`).
fn other_code<'a>(code: Option<&'a Code<'_>>) -> Option<&'a [u8]> {
    match code {
        Some(Code::Other(other)) => Some(other.inner()),
        Some(_) | None => None,
    }
}

/// Classifies a refused `AUTHENTICATE` by its response code (RFC 5530).
///
/// Only `AUTHENTICATIONFAILED`, or a refusal with no code, means the token is bad; every other
/// code (`UNAVAILABLE`, `LIMIT`, `INUSE`, Gmail's `ALERT` about too many connections) means the
/// server is busy and the sign-in should be retried later.
fn classify_refusal(code: Option<&Code<'_>>) -> ImapError {
    match (code, other_code(code)) {
        (None, _) => ImapError::AuthenticationFailed,
        (_, Some(name)) if name.eq_ignore_ascii_case(b"AUTHENTICATIONFAILED") => {
            ImapError::AuthenticationFailed
        }
        (Some(_), _) => ImapError::Unavailable,
    }
}

/// Classifies a refused command: busy (`UNAVAILABLE`, `LIMIT`, `INUSE`) or a protocol problem,
/// described by `refused`.
fn classify_command_refusal(code: Option<&Code<'_>>, refused: &'static str) -> ImapError {
    let busy = other_code(code).is_some_and(|name| {
        [b"UNAVAILABLE".as_slice(), b"LIMIT", b"INUSE"]
            .iter()
            .any(|busy| name.eq_ignore_ascii_case(busy))
    });
    if busy {
        ImapError::Unavailable
    } else {
        ImapError::Protocol(refused)
    }
}

/// Returns the name's bytes as the server sent them, quoting undone.
fn raw_name(mailbox: &Mailbox<'_>) -> Vec<u8> {
    match mailbox {
        Mailbox::Inbox => b"INBOX".to_vec(),
        Mailbox::Other(other) => match other.inner() {
            AString::Atom(atom) => atom.as_ref().as_bytes().to_vec(),
            AString::String(IString::Quoted(quoted)) => quoted.as_ref().as_bytes().to_vec(),
            AString::String(IString::Literal(literal)) => literal.as_ref().to_vec(),
        },
    }
}

/// Turns a listed name into a folder, or `None` when its name cannot be used (note 1.1 C6: such a
/// folder is skipped and the rest still sync).
fn remote_mailbox(
    attributes: &[FlagNameAttribute<'_>],
    delimiter: Option<char>,
    mailbox: &Mailbox<'_>,
) -> Option<RemoteMailbox> {
    let raw = raw_name(mailbox);
    let decoded = utf7::decode(std::str::from_utf8(&raw).ok()?)?;
    let path = MailboxPath::new(&decoded, delimiter).ok()?;
    let role = roles::role(&path, attributes);
    let selectable = roles::is_selectable(attributes);
    Some(RemoteMailbox::new(
        RemoteName::new(raw),
        path,
        role,
        selectable,
    ))
}

#[cfg(test)]
mod tests {
    use imap_next::imap_types::response::CodeOther;

    use super::*;

    fn other(name: &'static [u8]) -> Code<'static> {
        Code::Other(CodeOther::unvalidated(name))
    }

    #[test]
    fn only_authenticationfailed_or_no_code_means_a_bad_token() {
        assert!(
            matches!(classify_refusal(None), ImapError::AuthenticationFailed),
            "no code"
        );
        assert!(
            matches!(
                classify_refusal(Some(&other(b"AUTHENTICATIONFAILED"))),
                ImapError::AuthenticationFailed
            ),
            "AUTHENTICATIONFAILED"
        );
        for busy in [other(b"UNAVAILABLE"), other(b"LIMIT"), Code::Alert] {
            assert!(
                matches!(classify_refusal(Some(&busy)), ImapError::Unavailable),
                "{busy:?}"
            );
        }
    }

    #[test]
    fn a_busy_refusal_is_unavailable_and_others_are_protocol() {
        assert!(
            matches!(
                classify_command_refusal(Some(&other(b"UNAVAILABLE")), "refused"),
                ImapError::Unavailable
            ),
            "busy"
        );
        assert!(
            matches!(
                classify_command_refusal(None, "refused"),
                ImapError::Protocol(_)
            ),
            "plain NO"
        );
    }
}
