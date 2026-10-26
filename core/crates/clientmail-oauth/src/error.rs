use clientmail_domain::OAuthErrorKind;

/// Errors of the OAuth protocol. Messages never include tokens, codes or addresses.
#[derive(Debug, thiserror::Error)]
pub enum OAuthError {
    /// The user refused consent.
    #[error("the user refused consent")]
    Denied,
    /// The provider refused the grant (`invalid_grant`).
    #[error("the provider refused the grant")]
    Rejected,
    /// The provider could not be reached: no connection, a timeout, a TLS failure.
    #[error("the provider could not be reached")]
    Network(#[source] reqwest::Error),
    /// The provider answered with an OAuth error code other than `invalid_grant`.
    #[error("the provider returned error {code}")]
    Server {
        /// The `error` field of the response, for example `invalid_client`.
        code: String,
    },
    /// The provider's answer is malformed or fails a check.
    #[error("unexpected answer: {0}")]
    Protocol(&'static str),
    /// The configured client id is not a Google OAuth client id.
    #[error("not a Google OAuth client id")]
    InvalidClientId,
    /// The HTTP client could not be set up.
    #[error("the HTTP client could not be set up")]
    Setup(#[source] reqwest::Error),
    /// The operating system's random source failed.
    #[error("no random bytes available")]
    Random,
}

impl OAuthError {
    /// Classifies the error for the engine's rules.
    #[must_use]
    pub const fn kind(&self) -> OAuthErrorKind {
        match self {
            Self::Denied => OAuthErrorKind::Denied,
            Self::Rejected => OAuthErrorKind::Rejected,
            Self::Network(_) => OAuthErrorKind::Network,
            Self::Server { .. }
            | Self::Protocol(_)
            | Self::InvalidClientId
            | Self::Setup(_)
            | Self::Random => OAuthErrorKind::Protocol,
        }
    }
}
