use clientmail_domain::{AccessToken, SecretString, TokenGrant, UnixTime};
use serde::Deserialize;

use crate::{OAuthError, id_token};

/// The scope that grants IMAP and SMTP access to Gmail.
pub(crate) const MAIL_SCOPE: &str = "https://mail.google.com/";

/// A successful token response (RFC 6749, section 5.1). No `Debug`: it holds tokens.
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: i64,
    refresh_token: Option<String>,
    id_token: Option<String>,
    /// Space-separated granted scopes. Absent means the requested scopes were all granted
    /// (RFC 6749, section 5.1).
    scope: Option<String>,
}

/// An error response (RFC 6749, section 5.2).
#[derive(Deserialize)]
struct ErrorResponse {
    error: String,
}

/// Turns a token endpoint's answer into a grant.
///
/// # Arguments
///
/// * `status` - The HTTP status code.
/// * `body` - The response body.
/// * `client_id` - This app's client id, which an ID token must be issued to.
/// * `now` - The current time, for the access token's expiry and the ID token's checks.
///
/// # Errors
///
/// Returns [`OAuthError::Rejected`] for `invalid_grant`, [`OAuthError::Server`] for other error
/// codes, and [`OAuthError::Protocol`] for a malformed answer or a failed ID token check.
pub(crate) fn parse_response(
    status: u16,
    body: &[u8],
    client_id: &str,
    now: UnixTime,
) -> Result<TokenGrant, OAuthError> {
    if !(200..300).contains(&status) {
        return Err(error_from(body));
    }
    let response: TokenResponse = serde_json::from_slice(body)
        .map_err(|_| OAuthError::Protocol("token response is not valid JSON"))?;
    let identity = response
        .id_token
        .as_deref()
        .map(|token| id_token::identity(token, client_id, now))
        .transpose()?;
    let mail_access = response
        .scope
        .as_deref()
        .is_none_or(|scope| scope.split(' ').any(|granted| granted == MAIL_SCOPE));
    let access = AccessToken::new(
        SecretString::from(response.access_token),
        now.saturating_add_seconds(response.expires_in),
    );
    Ok(TokenGrant::new(
        access,
        response.refresh_token.map(SecretString::from),
        identity,
        mail_access,
    ))
}

fn error_from(body: &[u8]) -> OAuthError {
    match serde_json::from_slice::<ErrorResponse>(body) {
        Ok(response) if response.error == "invalid_grant" => OAuthError::Rejected,
        Ok(response) => OAuthError::Server {
            code: response.error,
        },
        Err(_) => OAuthError::Protocol("error response is not valid JSON"),
    }
}

#[cfg(test)]
mod tests {
    use clientmail_domain::ExposeSecret;
    use serde_json::json;

    use super::*;
    use crate::id_token::tests::{CLIENT_ID, token, valid_claims};

    const NOW: UnixTime = UnixTime::from_seconds(1_000);

    fn parse(status: u16, body: &serde_json::Value) -> Result<TokenGrant, OAuthError> {
        parse_response(status, body.to_string().as_bytes(), CLIENT_ID, NOW)
    }

    #[test]
    fn sign_in_response_gives_tokens_and_identity() {
        let body = json!({
            "access_token": "ya29.access",
            "expires_in": 3599,
            "refresh_token": "1//refresh",
            "scope": "https://mail.google.com/ openid email profile",
            "token_type": "Bearer",
            "id_token": token(&valid_claims()),
        });

        let grant = parse(200, &body).unwrap();

        assert_eq!(
            grant.access().secret().expose_secret(),
            "ya29.access",
            "access token"
        );
        assert_eq!(
            grant.access().expires_at().seconds(),
            4_599,
            "now + expires_in"
        );
        assert_eq!(
            grant.refresh().map(ExposeSecret::expose_secret),
            Some("1//refresh"),
            "refresh token"
        );
        assert_eq!(
            grant.identity().map(|identity| identity.email().as_str()),
            Some("ada@gmail.com"),
            "identity"
        );
    }

    #[test]
    fn mail_access_follows_the_granted_scope() {
        let grant = |scope: Option<&str>| {
            let mut body = json!({"access_token": "a", "expires_in": 1});
            if let Some(scope) = scope {
                body["scope"] = json!(scope);
            }
            parse(200, &body).unwrap().mail_access()
        };

        assert!(
            grant(Some("openid https://mail.google.com/ email")),
            "granted"
        );
        assert!(
            !grant(Some("openid email profile")),
            "unticked on the consent screen"
        );
        assert!(
            grant(None),
            "no scope field: everything requested was granted"
        );
    }

    #[test]
    fn refresh_response_has_no_refresh_token_or_identity() {
        let body = json!({"access_token": "ya29.new", "expires_in": 3599, "token_type": "Bearer"});

        let grant = parse(200, &body).unwrap();

        assert!(grant.refresh().is_none(), "no new refresh token");
        assert!(grant.identity().is_none(), "no identity");
    }

    #[test]
    fn invalid_grant_is_rejected_and_other_codes_are_kept() {
        let rejected = parse(400, &json!({"error": "invalid_grant"}));
        let other = parse(401, &json!({"error": "invalid_client"}));

        assert!(
            matches!(rejected, Err(OAuthError::Rejected)),
            "sign in again"
        );
        assert!(
            matches!(other, Err(OAuthError::Server { code }) if code == "invalid_client"),
            "code kept"
        );
    }

    #[test]
    fn malformed_answers_are_protocol_errors() {
        let not_json = parse_response(200, b"<html>", CLIENT_ID, NOW);
        let error_not_json = parse_response(500, b"oops", CLIENT_ID, NOW);
        let missing_field = parse(200, &json!({"expires_in": 1}));

        for result in [not_json, error_not_json, missing_field] {
            assert!(matches!(result, Err(OAuthError::Protocol(_))), "malformed");
        }
    }

    #[test]
    fn a_failed_id_token_check_fails_the_grant() {
        let mut claims = valid_claims();
        claims["aud"] = json!("someone-else");
        let body = json!({"access_token": "a", "expires_in": 1, "id_token": token(&claims)});

        assert!(
            matches!(parse(200, &body), Err(OAuthError::Protocol(_))),
            "rejected"
        );
    }
}
