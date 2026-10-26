use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use clientmail_domain::{EmailAddress, OAuthIdentity, UnixTime};
use serde::Deserialize;
use serde_json::Value;

use crate::OAuthError;

/// The issuers Google uses in ID tokens.
const GOOGLE_ISSUERS: [&str; 2] = ["https://accounts.google.com", "accounts.google.com"];
/// How far the Mac's clock may run ahead of Google's before a fresh token counts as expired.
const CLOCK_LEEWAY_SECONDS: i64 = 300;

/// The claims this crate reads. No `Debug`: it holds an address and a name.
#[derive(Deserialize)]
struct Claims {
    iss: String,
    aud: String,
    exp: i64,
    email: Option<String>,
    // Google sends a boolean; older tokens sent the string "true".
    email_verified: Option<Value>,
    name: Option<String>,
}

/// Reads who signed in from a Google ID token and checks it was meant for this client.
///
/// The signature is not checked: the token comes straight from Google's token endpoint over TLS,
/// which `OpenID` Connect Core 1.0, section 3.1.3.7, allows. The claims are checked.
///
/// # Arguments
///
/// * `token` - The `id_token` of a token response.
/// * `client_id` - This app's client id, which must be the audience.
/// * `now` - The current time, to check expiry.
///
/// # Errors
///
/// Returns [`OAuthError::Protocol`] when the token is malformed, comes from another issuer, was
/// issued to another client, has expired, or carries no verified address.
pub(crate) fn identity(
    token: &str,
    client_id: &str,
    now: UnixTime,
) -> Result<OAuthIdentity, OAuthError> {
    let payload = token
        .split('.')
        .nth(1)
        .ok_or(OAuthError::Protocol("ID token has no payload"))?;
    let bytes = URL_SAFE_NO_PAD
        .decode(payload.trim_end_matches('='))
        .map_err(|_| OAuthError::Protocol("ID token payload is not base64url"))?;
    let claims: Claims = serde_json::from_slice(&bytes)
        .map_err(|_| OAuthError::Protocol("ID token payload is not valid JSON"))?;
    if !GOOGLE_ISSUERS.contains(&claims.iss.as_str()) {
        return Err(OAuthError::Protocol("ID token from another issuer"));
    }
    if claims.aud != client_id {
        return Err(OAuthError::Protocol("ID token for another client"));
    }
    if claims.exp.saturating_add(CLOCK_LEEWAY_SECONDS) <= now.seconds() {
        return Err(OAuthError::Protocol("ID token has expired"));
    }
    let verified = match &claims.email_verified {
        Some(Value::Bool(verified)) => *verified,
        Some(Value::String(text)) => text == "true",
        _ => false,
    };
    if !verified {
        return Err(OAuthError::Protocol("address is not verified"));
    }
    let email = claims
        .email
        .as_deref()
        .ok_or(OAuthError::Protocol("ID token has no address"))
        .and_then(|email| {
            EmailAddress::parse(email)
                .map_err(|_| OAuthError::Protocol("ID token address is not valid"))
        })?;
    let name = claims
        .name
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty());
    Ok(OAuthIdentity::new(email, name))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const CLIENT_ID: &str = "1234-abcd.apps.googleusercontent.com";
    /// 2100-01-01, so test tokens never expire.
    const FAR_FUTURE: i64 = 4_102_444_800;

    /// Builds an unsigned ID token around `claims`, as Google's would look to this crate.
    pub(crate) fn token(claims: &Value) -> String {
        let encode = |value: &Value| URL_SAFE_NO_PAD.encode(value.to_string());
        format!(
            "{}.{}.signature",
            encode(&serde_json::json!({"alg": "RS256"})),
            encode(claims)
        )
    }

    pub(crate) fn valid_claims() -> Value {
        serde_json::json!({
            "iss": "https://accounts.google.com",
            "aud": CLIENT_ID,
            "exp": FAR_FUTURE,
            "email": "ada@gmail.com",
            "email_verified": true,
            "name": "Ada Lovelace",
        })
    }

    fn check(claims: &Value) -> Result<OAuthIdentity, OAuthError> {
        identity(&token(claims), CLIENT_ID, UnixTime::from_seconds(1_000))
    }

    #[test]
    fn valid_claims_give_the_identity() {
        let identity = check(&valid_claims()).unwrap();

        assert_eq!(identity.email().as_str(), "ada@gmail.com", "address");
        assert_eq!(identity.name(), Some("Ada Lovelace"), "name");
    }

    #[test]
    fn a_clock_up_to_five_minutes_fast_still_accepts_a_fresh_token() {
        let mut claims = valid_claims();
        claims["exp"] = Value::from(1_000);

        let four_minutes_late = identity(&token(&claims), CLIENT_ID, UnixTime::from_seconds(1_240));
        let six_minutes_late = identity(&token(&claims), CLIENT_ID, UnixTime::from_seconds(1_360));

        assert!(four_minutes_late.is_ok(), "within the leeway");
        assert!(
            matches!(six_minutes_late, Err(OAuthError::Protocol(_))),
            "expired"
        );
    }

    #[test]
    fn string_true_counts_as_verified() {
        let mut claims = valid_claims();
        claims["email_verified"] = Value::from("true");

        assert!(check(&claims).is_ok(), "older token format");
    }

    #[test]
    fn failed_checks_are_rejected() {
        let changes: [(&str, Value); 5] = [
            ("iss", Value::from("https://evil.example")),
            ("aud", Value::from("other.apps.googleusercontent.com")),
            ("exp", Value::from(600)),
            ("email_verified", Value::from(false)),
            ("email", Value::from("not an address")),
        ];

        for (field, value) in changes {
            let mut claims = valid_claims();
            claims[field] = value;

            assert!(
                matches!(check(&claims), Err(OAuthError::Protocol(_))),
                "rejected when {field} is wrong"
            );
        }
    }

    #[test]
    fn malformed_tokens_are_rejected() {
        for token in ["", "only-one-part", "a.!!!.c", "a.e30.c"] {
            assert!(
                matches!(
                    identity(token, CLIENT_ID, UnixTime::from_seconds(0)),
                    Err(OAuthError::Protocol(_))
                ),
                "rejected: {token}"
            );
        }
    }
}
