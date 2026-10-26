use clientmail_domain::MailErrorKind;

/// Errors of the IMAP client.
///
/// Messages are fixed text: they never include the token, the address, folder names or anything
/// else the server sent, and no error keeps a library error, which can carry raw server bytes
/// (decision record 0011).
#[derive(Debug, thiserror::Error)]
pub enum ImapError {
    /// The server could not be reached, the TLS handshake failed, or the connection broke.
    #[error("the mail server could not be reached")]
    Network,
    /// The server did not answer in time.
    #[error("the mail server did not answer in time")]
    Timeout,
    /// An earlier call failed, timed out or was cancelled, so this session cannot be trusted.
    #[error("the mail session is broken; connect again")]
    Broken,
    /// The server refused the access token (`[AUTHENTICATIONFAILED]`, or a refusal without a
    /// code).
    #[error("the mail server refused the sign-in")]
    AuthenticationFailed,
    /// The server is busy or temporarily refusing: a `BYE` greeting, `[UNAVAILABLE]`,
    /// `[LIMIT]`, `[INUSE]` or an `[ALERT]`.
    #[error("the mail server is temporarily unavailable")]
    Unavailable,
    /// The server does not offer XOAUTH2, so the token was not sent.
    #[error("the mail server does not support signing in with OAuth")]
    NoXoauth2,
    /// A response could not be parsed; it was discarded.
    #[error("a response from the mail server could not be read")]
    Unparsable,
    /// The server answered something this client cannot use.
    #[error("unexpected answer from the mail server: {0}")]
    Protocol(&'static str),
    /// The TLS configuration could not be built.
    #[error("TLS could not be set up")]
    Tls,
}

impl ImapError {
    /// Classifies the error for the engine's rules.
    #[must_use]
    pub const fn kind(&self) -> MailErrorKind {
        match self {
            Self::Network | Self::Timeout | Self::Broken => MailErrorKind::Network,
            Self::AuthenticationFailed => MailErrorKind::AuthenticationFailed,
            Self::Unavailable => MailErrorKind::Unavailable,
            Self::NoXoauth2 | Self::Unparsable | Self::Protocol(_) | Self::Tls => {
                MailErrorKind::Protocol
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_follow_the_rules() {
        let cases = [
            (ImapError::Timeout, MailErrorKind::Network),
            (ImapError::Broken, MailErrorKind::Network),
            (
                ImapError::AuthenticationFailed,
                MailErrorKind::AuthenticationFailed,
            ),
            (ImapError::Unavailable, MailErrorKind::Unavailable),
            (ImapError::NoXoauth2, MailErrorKind::Protocol),
            (ImapError::Unparsable, MailErrorKind::Protocol),
        ];

        for (error, kind) in cases {
            assert_eq!(error.kind(), kind, "{error:?}");
        }
    }
}
