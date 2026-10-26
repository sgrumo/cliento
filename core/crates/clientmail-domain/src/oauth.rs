use std::fmt;

use secrecy::{ExposeSecret, SecretString};

use crate::redact::redact_option;
use crate::{DomainError, EmailAddress, UnixTime};

/// The PKCE code verifier of one authorization (RFC 7636). Secret until the code is exchanged.
#[derive(Debug)]
pub struct PkceVerifier(SecretString);

impl PkceVerifier {
    /// Validates a verifier.
    ///
    /// # Arguments
    ///
    /// * `verifier` - 43 to 128 characters from `A-Z a-z 0-9 - . _ ~`.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::InvalidPkceVerifier`] when the length or a character is not allowed.
    pub fn new(verifier: SecretString) -> Result<Self, DomainError> {
        let text = verifier.expose_secret();
        let unreserved = |character: char| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '.' | '_' | '~')
        };
        if (43..=128).contains(&text.len()) && text.chars().all(unreserved) {
            Ok(Self(verifier))
        } else {
            Err(DomainError::InvalidPkceVerifier)
        }
    }

    /// Returns the verifier.
    #[must_use]
    pub const fn secret(&self) -> &SecretString {
        &self.0
    }
}

/// The random `state` of one authorization. The callback must return the same value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthState(String);

impl OAuthState {
    /// Wraps a state value.
    ///
    /// # Arguments
    ///
    /// * `state` - The value sent with the authorization, or the one the callback returned.
    #[must_use]
    pub const fn new(state: String) -> Self {
        Self(state)
    }

    /// Returns the value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The one-time code the provider returns to the callback.
#[derive(Debug)]
pub struct AuthorizationCode(SecretString);

impl AuthorizationCode {
    /// Wraps a code.
    ///
    /// # Arguments
    ///
    /// * `code` - The `code` parameter of the callback.
    #[must_use]
    pub const fn new(code: SecretString) -> Self {
        Self(code)
    }

    /// Returns the code.
    #[must_use]
    pub const fn secret(&self) -> &SecretString {
        &self.0
    }
}

/// What starting an authorization produces: the URL to open, and what the callback is checked
/// against.
#[derive(Debug)]
pub struct AuthorizationRequest {
    url: String,
    state: OAuthState,
    verifier: PkceVerifier,
}

impl AuthorizationRequest {
    /// Creates a request.
    ///
    /// # Arguments
    ///
    /// * `url` - The provider's authorization URL with every parameter.
    /// * `state` - The state the URL carries.
    /// * `verifier` - The verifier whose challenge the URL carries.
    #[must_use]
    pub const fn new(url: String, state: OAuthState, verifier: PkceVerifier) -> Self {
        Self {
            url,
            state,
            verifier,
        }
    }

    /// Returns the URL to open in the browser.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Returns the expected state.
    #[must_use]
    pub const fn state(&self) -> &OAuthState {
        &self.state
    }

    /// Takes the request apart, so the verifier can move into the code exchange.
    #[must_use]
    pub fn into_parts(self) -> (String, OAuthState, PkceVerifier) {
        (self.url, self.state, self.verifier)
    }
}

/// A short-lived token for the mail server, kept in memory only.
#[derive(Debug, Clone)]
pub struct AccessToken {
    token: SecretString,
    expires_at: UnixTime,
}

impl AccessToken {
    /// Creates a token.
    ///
    /// # Arguments
    ///
    /// * `token` - The access token.
    /// * `expires_at` - When the provider stops accepting it.
    #[must_use]
    pub const fn new(token: SecretString, expires_at: UnixTime) -> Self {
        Self { token, expires_at }
    }

    /// Returns the token.
    #[must_use]
    pub const fn secret(&self) -> &SecretString {
        &self.token
    }

    /// Returns when the token expires.
    #[must_use]
    pub const fn expires_at(&self) -> UnixTime {
        self.expires_at
    }

    /// Returns whether the token is still valid `margin_seconds` after `now`.
    ///
    /// # Arguments
    ///
    /// * `now` - The current time.
    /// * `margin_seconds` - How long the caller needs the token to stay valid.
    #[must_use]
    pub const fn is_valid_for(&self, now: UnixTime, margin_seconds: i64) -> bool {
        now.saturating_add_seconds(margin_seconds).seconds() < self.expires_at.seconds()
    }
}

/// Who signed in, as the provider reports it. `Debug` hides personal data.
#[derive(Clone, PartialEq, Eq)]
pub struct OAuthIdentity {
    email: EmailAddress,
    name: Option<String>,
}

impl OAuthIdentity {
    /// Creates an identity.
    ///
    /// # Arguments
    ///
    /// * `email` - The verified address of the signed-in account.
    /// * `name` - The account's display name, when the provider sends one.
    #[must_use]
    pub const fn new(email: EmailAddress, name: Option<String>) -> Self {
        Self { email, name }
    }

    /// Returns the signed-in address.
    #[must_use]
    pub const fn email(&self) -> &EmailAddress {
        &self.email
    }

    /// Returns the display name, if any.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
}

impl fmt::Debug for OAuthIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthIdentity")
            .field("email", &self.email)
            .field("name", &redact_option(self.name.as_ref()))
            .finish()
    }
}

/// What a token endpoint returns for a code exchange or a refresh.
#[derive(Debug)]
pub struct TokenGrant {
    access: AccessToken,
    refresh: Option<SecretString>,
    identity: Option<OAuthIdentity>,
    mail_access: bool,
}

impl TokenGrant {
    /// Creates a grant.
    ///
    /// # Arguments
    ///
    /// * `access` - The new access token.
    /// * `refresh` - A refresh token, when the provider issued one.
    /// * `identity` - Who signed in, when the grant carried an ID token.
    /// * `mail_access` - Whether the grant includes access to mail. Providers let the user refuse
    ///   single permissions while still signing in.
    #[must_use]
    pub const fn new(
        access: AccessToken,
        refresh: Option<SecretString>,
        identity: Option<OAuthIdentity>,
        mail_access: bool,
    ) -> Self {
        Self {
            access,
            refresh,
            identity,
            mail_access,
        }
    }

    /// Returns whether the grant includes access to mail.
    #[must_use]
    pub const fn mail_access(&self) -> bool {
        self.mail_access
    }

    /// Returns the access token.
    #[must_use]
    pub const fn access(&self) -> &AccessToken {
        &self.access
    }

    /// Returns the refresh token, if one was issued.
    #[must_use]
    pub const fn refresh(&self) -> Option<&SecretString> {
        self.refresh.as_ref()
    }

    /// Returns who signed in, if the grant said.
    #[must_use]
    pub const fn identity(&self) -> Option<&OAuthIdentity> {
        self.identity.as_ref()
    }

    /// Takes the grant apart.
    #[must_use]
    pub fn into_parts(self) -> (AccessToken, Option<SecretString>, Option<OAuthIdentity>) {
        (self.access, self.refresh, self.identity)
    }
}

/// How an OAuth operation failed, as far as the engine's rules care.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OAuthErrorKind {
    /// The user refused consent.
    Denied,
    /// The provider refused the grant (`invalid_grant`): the user must sign in again.
    Rejected,
    /// The provider could not be reached: no connection, a timeout, a TLS failure.
    Network,
    /// Anything malformed or unexpected.
    Protocol,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verifier(text: &str) -> Result<PkceVerifier, DomainError> {
        PkceVerifier::new(SecretString::from(text))
    }

    #[test]
    fn verifier_accepts_the_rfc_alphabet_and_lengths() {
        let shortest = "a".repeat(43);
        let longest = "Az09-._~".repeat(16);

        assert!(verifier(&shortest).is_ok(), "43 characters");
        assert!(verifier(&longest).is_ok(), "128 characters");
    }

    #[test]
    fn verifier_rejects_bad_lengths_and_characters() {
        assert_eq!(
            verifier(&"a".repeat(42)).err(),
            Some(DomainError::InvalidPkceVerifier),
            "too short"
        );
        assert_eq!(
            verifier(&"a".repeat(129)).err(),
            Some(DomainError::InvalidPkceVerifier),
            "too long"
        );
        assert_eq!(
            verifier(&format!("{}+", "a".repeat(42))).err(),
            Some(DomainError::InvalidPkceVerifier),
            "reserved character"
        );
    }

    #[test]
    fn access_token_validity_honours_the_margin() {
        let token = AccessToken::new(SecretString::from("t"), UnixTime::from_seconds(1_000));

        assert!(
            token.is_valid_for(UnixTime::from_seconds(900), 60),
            "100 s left"
        );
        assert!(
            !token.is_valid_for(UnixTime::from_seconds(940), 60),
            "exactly 60 s left"
        );
        assert!(
            !token.is_valid_for(UnixTime::from_seconds(2_000), 0),
            "expired"
        );
    }

    #[test]
    fn debug_output_hides_secrets_and_personal_data() {
        // Every secret or personal value contains "leak"; `SecretString` itself prints
        // `SecretBox<str>([REDACTED])`, so the check cannot look for the word "secret".
        let identity = OAuthIdentity::new(
            EmailAddress::parse("leak@leak.example").unwrap(),
            Some("Leak Name".to_owned()),
        );
        let grant = TokenGrant::new(
            AccessToken::new(SecretString::from("leak-access"), UnixTime::from_seconds(1)),
            Some(SecretString::from("leak-refresh")),
            Some(identity),
            true,
        );
        let code = AuthorizationCode::new(SecretString::from("leak-code"));
        let request = AuthorizationRequest::new(
            "https://example.com".to_owned(),
            OAuthState::new("state".to_owned()),
            verifier(&"leak".repeat(11)).unwrap(),
        );

        let outputs = [
            format!("{grant:?}"),
            format!("{code:?}"),
            format!("{request:?}"),
        ];

        for output in outputs {
            assert!(!output.to_lowercase().contains("leak"), "leaked: {output}");
        }
    }
}
