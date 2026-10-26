use std::sync::Arc;
use std::time::Duration;

use clientmail_domain::ImapServer;
use rustls::ClientConfig;
use rustls::pki_types::ServerName;
use rustls_platform_verifier::BuilderVerifierExt as _;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;

use crate::ImapError;

/// How long the TCP connection and the TLS handshake may take together.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// Opens TLS connections with the same stack and trust settings as HTTPS: rustls with the
/// aws-lc-rs provider, certificates checked by macOS (decision record 0010).
#[derive(Clone)]
pub(crate) struct Tls {
    connector: TlsConnector,
}

impl Tls {
    /// Builds the TLS configuration.
    ///
    /// # Errors
    ///
    /// Returns [`ImapError::Tls`] when the platform verifier cannot be set up.
    pub(crate) fn new() -> Result<Self, ImapError> {
        // The provider is named explicitly: with more than one compiled in, rustls cannot pick a
        // default and would panic.
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let mut config = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|_| ImapError::Tls)?
            .with_platform_verifier()
            .map_err(|_| ImapError::Tls)?
            .with_no_client_auth();
        // ALPN `imap` (IANA registry) lets a server reject this connection if it expects another
        // protocol, which blunts cross-protocol attacks such as ALPACA; servers that do not use
        // ALPN ignore it.
        config.alpn_protocols = vec![b"imap".to_vec()];
        Ok(Self {
            connector: TlsConnector::from(Arc::new(config)),
        })
    }

    /// Connects to `server` and completes the TLS handshake (implicit TLS, as on port 993).
    ///
    /// # Errors
    ///
    /// Returns [`ImapError::Timeout`] after 30 s, [`ImapError::Network`] when the connection or
    /// the handshake fails (an untrusted certificate included), and [`ImapError::Protocol`] for a
    /// host that is not a valid DNS name.
    pub(crate) async fn connect(
        &self,
        server: &ImapServer,
    ) -> Result<TlsStream<TcpStream>, ImapError> {
        let name = ServerName::try_from(server.host().to_owned())
            .map_err(|_| ImapError::Protocol("the server's host name is not valid"))?;
        let connect = async {
            let tcp = TcpStream::connect((server.host(), server.port())).await?;
            self.connector.connect(name, tcp).await
        };
        tokio::time::timeout(CONNECT_TIMEOUT, connect)
            .await
            .map_err(|_| ImapError::Timeout)?
            .map_err(|_| ImapError::Network)
    }
}

impl std::fmt::Debug for Tls {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Tls")
    }
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;

    use super::*;

    #[test]
    fn the_configuration_builds_with_the_platform_verifier() {
        assert!(Tls::new().is_ok(), "TLS config");
    }

    #[tokio::test]
    async fn an_invalid_host_name_is_a_protocol_error() {
        let tls = Tls::new().unwrap();

        let result = tls
            .connect(&ImapServer::new("not a host!".to_owned(), 993))
            .await;

        assert!(matches!(result, Err(ImapError::Protocol(_))), "{result:?}");
    }

    #[tokio::test]
    async fn a_refused_connection_is_a_network_error() {
        // A port that was free a moment ago: nothing listens on it now.
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let tls = Tls::new().unwrap();

        let result = tls
            .connect(&ImapServer::new("localhost".to_owned(), port))
            .await;

        assert!(matches!(result, Err(ImapError::Network)), "{result:?}");
    }
}
