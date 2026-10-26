//! IMAP over TLS for the `ClientMail` core: XOAUTH2 sign-in and folder listing.
//!
//! [`ImapConnector`] implements the domain's [`clientmail_domain::ports::MailConnector`] on top of
//! `imap-next`'s sans-I/O client and `imap-codec` (decision record 0011). It holds protocol
//! details only. Errors carry fixed messages, never server text, tokens or addresses.

mod error;
mod fetch;
mod roles;
mod session;
mod tls;
mod utf7;

use clientmail_domain::ports::MailConnector;
use clientmail_domain::{EmailAddress, ImapServer, MailErrorKind, SecretString};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;

pub use error::ImapError;
pub use session::ImapSession;

use crate::tls::Tls;

/// Opens authenticated IMAP sessions over TLS.
#[derive(Debug, Clone)]
pub struct ImapConnector {
    tls: Tls,
}

impl ImapConnector {
    /// Creates the connector and its TLS configuration.
    ///
    /// # Errors
    ///
    /// Returns [`ImapError::Tls`] when the platform's certificate verifier cannot be set up.
    pub fn new() -> Result<Self, ImapError> {
        Ok(Self { tls: Tls::new()? })
    }
}

impl MailConnector for ImapConnector {
    type Error = ImapError;
    type Session = ImapSession<TlsStream<TcpStream>>;

    fn error_kind(error: &ImapError) -> MailErrorKind {
        error.kind()
    }

    async fn connect(
        &self,
        server: &ImapServer,
        user: &EmailAddress,
        access_token: &SecretString,
    ) -> Result<Self::Session, ImapError> {
        let stream = self.tls.connect(server).await?;
        ImapSession::open(stream, user, access_token).await
    }
}
