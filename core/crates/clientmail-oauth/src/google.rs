use std::time::{SystemTime, UNIX_EPOCH};

use clientmail_domain::ports::OAuthClient;
use clientmail_domain::{
    AuthorizationCode, AuthorizationRequest, ExposeSecret, OAuthErrorKind, OAuthState,
    PkceVerifier, SecretString, TokenGrant, UnixTime,
};
use url::Url;

use crate::http::Http;
use crate::{OAuthError, callback, pkce, token};

const AUTHORIZATION_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const CLIENT_ID_SUFFIX: &str = ".apps.googleusercontent.com";
const SCHEME_PREFIX: &str = "com.googleusercontent.apps.";
const REDIRECT_PATH: &str = ":/oauth2redirect";
/// IMAP and SMTP access, plus the signed-in address and name for the new account.
const SCOPES: &str = "https://mail.google.com/ openid email profile";

/// Google's OAuth 2.0 for an iOS-type client: no client secret, PKCE, and the reversed client id
/// as the redirect scheme (`docs/google-oauth.md`).
#[derive(Debug)]
pub struct GoogleOAuth {
    client_id: String,
    scheme: String,
    redirect_uri: Url,
    token_url: Url,
    http: Http,
}

impl GoogleOAuth {
    /// Creates the client.
    ///
    /// # Arguments
    ///
    /// * `client_id` - The OAuth client id, `<number>-<hash>.apps.googleusercontent.com`.
    ///
    /// # Errors
    ///
    /// Returns [`OAuthError::InvalidClientId`] for anything that is not a Google client id, and
    /// [`OAuthError::Setup`] when the HTTP client cannot be built.
    pub fn new(client_id: &str) -> Result<Self, OAuthError> {
        Self::with_parts(client_id, TOKEN_URL, Http::new()?)
    }

    fn with_parts(client_id: &str, token_url: &str, http: Http) -> Result<Self, OAuthError> {
        let client_id = client_id.trim();
        let prefix = client_id
            .strip_suffix(CLIENT_ID_SUFFIX)
            .filter(|prefix| {
                !prefix.is_empty()
                    && prefix
                        .chars()
                        .all(|character| character.is_ascii_alphanumeric() || character == '-')
            })
            .ok_or(OAuthError::InvalidClientId)?;
        // URL schemes are case-insensitive and `url` lowercases them when parsing the callback.
        let scheme = format!("{SCHEME_PREFIX}{}", prefix.to_ascii_lowercase());
        let token_url =
            Url::parse(token_url).map_err(|_| OAuthError::Protocol("token URL is not valid"))?;
        let redirect_uri = Url::parse(&format!("{scheme}{REDIRECT_PATH}"))
            .map_err(|_| OAuthError::InvalidClientId)?;
        Ok(Self {
            client_id: client_id.to_owned(),
            redirect_uri,
            scheme,
            token_url,
            http,
        })
    }

    async fn post(&self, form: &[(&str, &str)]) -> Result<TokenGrant, OAuthError> {
        let (status, body) = self.http.post_form(&self.token_url, form).await?;
        token::parse_response(status, &body, &self.client_id, now())
    }
}

impl OAuthClient for GoogleOAuth {
    type Error = OAuthError;

    fn error_kind(error: &OAuthError) -> OAuthErrorKind {
        error.kind()
    }

    fn authorization(&self) -> Result<AuthorizationRequest, OAuthError> {
        let verifier = PkceVerifier::new(SecretString::from(pkce::random_token()?))
            .map_err(|_| OAuthError::Protocol("generated verifier is not valid"))?;
        let state = OAuthState::new(pkce::random_token()?);
        let url = Url::parse_with_params(
            AUTHORIZATION_URL,
            [
                ("client_id", self.client_id.as_str()),
                ("redirect_uri", self.redirect_uri.as_str()),
                ("response_type", "code"),
                ("scope", SCOPES),
                ("code_challenge", &pkce::challenge(&verifier)),
                ("code_challenge_method", "S256"),
                ("state", state.as_str()),
            ],
        )
        .map_err(|_| OAuthError::Protocol("authorization URL is not valid"))?;
        Ok(AuthorizationRequest::new(url.into(), state, verifier))
    }

    fn callback_scheme(&self) -> &str {
        &self.scheme
    }

    fn parse_callback(
        &self,
        url: &str,
        expected: &OAuthState,
    ) -> Result<AuthorizationCode, OAuthError> {
        callback::parse(url, &self.redirect_uri, expected)
    }

    async fn exchange(
        &self,
        code: AuthorizationCode,
        verifier: PkceVerifier,
    ) -> Result<TokenGrant, OAuthError> {
        self.post(&[
            ("grant_type", "authorization_code"),
            ("code", code.secret().expose_secret()),
            ("code_verifier", verifier.secret().expose_secret()),
            ("client_id", &self.client_id),
            ("redirect_uri", self.redirect_uri.as_str()),
        ])
        .await
    }

    async fn refresh(&self, refresh_token: SecretString) -> Result<TokenGrant, OAuthError> {
        self.post(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token.expose_secret()),
            ("client_id", &self.client_id),
        ])
        .await
    }
}

fn now() -> UnixTime {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    UnixTime::from_seconds(i64::try_from(seconds).unwrap_or(i64::MAX))
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::thread::{self, JoinHandle};

    use serde_json::json;

    use super::*;
    use crate::id_token::tests::{CLIENT_ID, token, valid_claims};

    fn google() -> GoogleOAuth {
        GoogleOAuth::new(CLIENT_ID).unwrap()
    }

    /// Google's client pointed at a local token endpoint, without proxies.
    fn local(token_url: &str) -> GoogleOAuth {
        GoogleOAuth::with_parts(CLIENT_ID, token_url, Http::without_proxy().unwrap()).unwrap()
    }

    /// Answers one HTTP request with `status` and `body`, and returns the request body it read.
    fn serve_once(status: u16, body: String) -> (String, JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/token", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap();
                }
            }
            let mut request = vec![0; length];
            reader.read_exact(&mut request).unwrap();
            let mut stream = stream;
            write!(
                stream,
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            String::from_utf8(request).unwrap()
        });
        (url, server)
    }

    /// reqwest needs a Tokio reactor; the core's runtime provides one in the app.
    fn run<T>(future: impl Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(future)
    }

    #[test]
    fn redirect_uses_the_reversed_client_id() {
        let google = google();

        assert_eq!(
            google.callback_scheme(),
            "com.googleusercontent.apps.1234-abcd",
            "scheme"
        );
        assert_eq!(
            google.redirect_uri.as_str(),
            "com.googleusercontent.apps.1234-abcd:/oauth2redirect",
            "redirect"
        );
        assert!(
            SCOPES.split(' ').any(|scope| scope == token::MAIL_SCOPE),
            "mail access is requested"
        );
    }

    #[test]
    fn other_client_ids_are_refused() {
        for id in [
            "",
            "1234-abcd",
            ".apps.googleusercontent.com",
            "a b.apps.googleusercontent.com",
        ] {
            assert!(
                matches!(GoogleOAuth::new(id), Err(OAuthError::InvalidClientId)),
                "refused: {id:?}"
            );
        }
    }

    #[test]
    fn authorization_url_carries_every_parameter() {
        let request = google().authorization().unwrap();
        let url = Url::parse(request.url()).unwrap();
        let query: Vec<(String, String)> = url.query_pairs().into_owned().collect();
        let get = |key: &str| {
            query
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.clone())
        };
        let expected_challenge = {
            let (_, _, verifier) = request.into_parts();
            pkce::challenge(&verifier)
        };

        assert_eq!(url.host_str(), Some("accounts.google.com"), "host");
        assert_eq!(get("client_id").as_deref(), Some(CLIENT_ID), "client id");
        assert_eq!(
            get("redirect_uri").as_deref(),
            Some("com.googleusercontent.apps.1234-abcd:/oauth2redirect"),
            "redirect"
        );
        assert_eq!(get("response_type").as_deref(), Some("code"), "code flow");
        assert_eq!(get("scope").as_deref(), Some(SCOPES), "scopes");
        assert_eq!(
            get("code_challenge_method").as_deref(),
            Some("S256"),
            "S256"
        );
        assert_eq!(
            get("code_challenge"),
            Some(expected_challenge),
            "challenge of the verifier"
        );
        assert!(get("state").is_some_and(|state| state.len() == 43), "state");
    }

    #[test]
    fn two_authorizations_use_different_state_and_verifier() {
        let google = google();

        let (first_url, first_state, _) = google.authorization().unwrap().into_parts();
        let (second_url, second_state, _) = google.authorization().unwrap().into_parts();

        assert_ne!(first_state, second_state, "state");
        assert_ne!(first_url, second_url, "challenge and state differ");
    }

    #[test]
    fn exchange_posts_the_code_and_verifier_and_parses_the_grant() {
        let body = json!({
            "access_token": "ya29.access",
            "expires_in": 3599,
            "refresh_token": "1//refresh",
            "id_token": token(&valid_claims()),
        });
        let (url, server) = serve_once(200, body.to_string());
        let google = local(&url);
        let verifier = PkceVerifier::new(SecretString::from("v".repeat(43))).unwrap();
        let code = AuthorizationCode::new(SecretString::from("4/0code"));

        let grant = run(google.exchange(code, verifier)).unwrap();
        let request = server.join().unwrap();

        assert!(
            request.contains("grant_type=authorization_code"),
            "{request}"
        );
        assert!(request.contains("code=4%2F0code"), "{request}");
        assert!(
            request.contains(&format!("code_verifier={}", "v".repeat(43))),
            "{request}"
        );
        assert!(
            request.contains("client_id=1234-abcd.apps.googleusercontent.com"),
            "{request}"
        );
        assert!(!request.contains("client_secret"), "no secret: {request}");
        assert!(grant.refresh().is_some(), "refresh token");
        assert!(grant.identity().is_some(), "identity");
    }

    #[test]
    fn refresh_posts_the_refresh_token() {
        let (url, server) = serve_once(400, json!({"error": "invalid_grant"}).to_string());
        let google = local(&url);

        let result = run(google.refresh(SecretString::from("1//refresh")));
        let request = server.join().unwrap();

        assert!(request.contains("grant_type=refresh_token"), "{request}");
        assert!(
            request.contains("refresh_token=1%2F%2Frefresh"),
            "{request}"
        );
        assert!(matches!(result, Err(OAuthError::Rejected)), "invalid_grant");
    }

    #[test]
    fn a_refused_connection_is_a_network_error() {
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let google = local(&format!("http://127.0.0.1:{port}/token"));

        let result = run(google.refresh(SecretString::from("1//refresh")));

        assert_eq!(
            result.err().map(|error| error.kind()),
            Some(OAuthErrorKind::Network),
            "network"
        );
    }
}
