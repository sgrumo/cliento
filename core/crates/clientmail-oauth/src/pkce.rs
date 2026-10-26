use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use clientmail_domain::{ExposeSecret, PkceVerifier};
use sha2::{Digest, Sha256};

use crate::OAuthError;

/// Returns 32 random bytes as base64url: 43 characters, valid both as a PKCE verifier and as a
/// state value.
///
/// # Errors
///
/// Returns [`OAuthError::Random`] when the operating system's random source fails.
pub(crate) fn random_token() -> Result<String, OAuthError> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| OAuthError::Random)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

/// Returns the `S256` code challenge of a verifier (RFC 7636, section 4.2).
pub(crate) fn challenge(verifier: &PkceVerifier) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.secret().expose_secret().as_bytes()))
}

#[cfg(test)]
mod tests {
    use clientmail_domain::SecretString;

    use super::*;

    #[test]
    fn challenge_matches_rfc_7636_appendix_b() {
        let verifier = PkceVerifier::new(SecretString::from(
            "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk",
        ))
        .unwrap();

        let challenge = challenge(&verifier);

        assert_eq!(
            challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM",
            "RFC example"
        );
    }

    #[test]
    fn random_tokens_are_valid_verifiers_and_differ() {
        let first = random_token().unwrap();
        let second = random_token().unwrap();

        assert_eq!(first.len(), 43, "32 bytes in base64url");
        assert!(
            PkceVerifier::new(SecretString::from(first.clone())).is_ok(),
            "valid verifier"
        );
        assert_ne!(first, second, "random");
    }
}
