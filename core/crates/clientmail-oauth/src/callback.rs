use clientmail_domain::{AuthorizationCode, OAuthState, SecretString};
use url::Url;

use crate::OAuthError;

/// Reads the authorization code out of a callback (RFC 6749, section 4.1.2), after checking it
/// belongs to this authorization.
///
/// The scheme, the path and the state are checked first, error callbacks included, so a stray
/// callback can neither complete nor end the sign-in (RFC 6749, section 10.12).
///
/// # Arguments
///
/// * `url` - The URL the browser was redirected to.
/// * `redirect_uri` - The redirect URI sent with the authorization; the callback must match its
///   scheme and path.
/// * `expected` - The state sent with the authorization.
///
/// # Errors
///
/// Returns [`OAuthError::Denied`] for `error=access_denied`, [`OAuthError::Server`] for other
/// error codes, and [`OAuthError::Protocol`] for a URL that is not a complete callback of this
/// authorization.
pub(crate) fn parse(
    url: &str,
    redirect_uri: &Url,
    expected: &OAuthState,
) -> Result<AuthorizationCode, OAuthError> {
    let url = Url::parse(url).map_err(|_| OAuthError::Protocol("callback is not a URL"))?;
    if url.scheme() != redirect_uri.scheme() || url.path() != redirect_uri.path() {
        return Err(OAuthError::Protocol("callback is not the redirect URI"));
    }
    let (mut code, mut state, mut error) = (None, None, None);
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.into_owned()),
            "state" => state = Some(value.into_owned()),
            "error" => error = Some(value.into_owned()),
            _ => {}
        }
    }
    if state.as_deref() != Some(expected.as_str()) {
        return Err(OAuthError::Protocol("callback belongs to another sign-in"));
    }
    if let Some(code) = error {
        return Err(if code == "access_denied" {
            OAuthError::Denied
        } else {
            OAuthError::Server { code }
        });
    }
    code.map(|code| AuthorizationCode::new(SecretString::from(code)))
        .ok_or(OAuthError::Protocol("callback without code"))
}

#[cfg(test)]
mod tests {
    use clientmail_domain::ExposeSecret;

    use super::*;

    const REDIRECT: &str = "com.googleusercontent.apps.1234-abcd:/oauth2redirect";

    fn check(url: &str) -> Result<AuthorizationCode, OAuthError> {
        parse(
            url,
            &Url::parse(REDIRECT).unwrap(),
            &OAuthState::new("s+1".to_owned()),
        )
    }

    #[test]
    fn the_code_of_a_matching_callback_is_read() {
        let code = check(&format!("{REDIRECT}?state=s%2B1&code=4%2F0abc&scope=email")).unwrap();

        assert_eq!(code.secret().expose_secret(), "4/0abc", "code decoded");
    }

    #[test]
    fn access_denied_is_denied_and_other_errors_keep_their_code() {
        let denied = check(&format!("{REDIRECT}?error=access_denied&state=s%2B1"));
        let other = check(&format!("{REDIRECT}?error=invalid_scope&state=s%2B1"));

        assert!(matches!(denied, Err(OAuthError::Denied)), "denied");
        assert!(
            matches!(other, Err(OAuthError::Server { code }) if code == "invalid_scope"),
            "server code kept"
        );
    }

    #[test]
    fn callbacks_of_another_sign_in_are_refused_even_when_they_report_an_error() {
        let cases = [
            format!("{REDIRECT}?code=abc&state=other"),
            format!("{REDIRECT}?code=abc"),
            format!("{REDIRECT}?error=access_denied&state=other"),
            format!("{REDIRECT}?error=access_denied"),
        ];

        for case in cases {
            assert!(
                matches!(check(&case), Err(OAuthError::Protocol(_))),
                "refused: {case}"
            );
        }
    }

    #[test]
    fn incomplete_or_foreign_callbacks_are_protocol_errors() {
        let cases = [
            format!("{REDIRECT}?state=s%2B1"),
            "com.googleusercontent.apps.1234-abcd:/elsewhere?code=a&state=s%2B1".to_owned(),
            "https://evil.example/oauth2redirect?code=a&state=s%2B1".to_owned(),
            "not a url".to_owned(),
        ];

        for case in cases {
            assert!(
                matches!(check(&case), Err(OAuthError::Protocol(_))),
                "rejected: {case}"
            );
        }
    }
}
