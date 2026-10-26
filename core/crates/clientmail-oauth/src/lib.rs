//! OAuth protocols for the `ClientMail` core.
//!
//! [`GoogleOAuth`] implements the domain's [`clientmail_domain::ports::OAuthClient`] port for
//! Google: the authorization URL with PKCE, the callback, and the token endpoint over HTTPS
//! (decision record 0010). It holds protocol details only; the engine decides what to store.

mod callback;
mod error;
mod google;
mod http;
mod id_token;
mod pkce;
mod token;

pub use error::OAuthError;
pub use google::GoogleOAuth;
