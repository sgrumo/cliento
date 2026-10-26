use std::time::Duration;

use url::Url;

use crate::OAuthError;

/// How long one request may take, connection included.
const TIMEOUT: Duration = Duration::from_secs(30);

/// A small HTTPS client: rustls with macOS trust settings, the system proxy settings, and a
/// timeout (decision record 0010).
#[derive(Debug)]
pub(crate) struct Http {
    client: reqwest::Client,
}

impl Http {
    /// Builds the client.
    ///
    /// # Errors
    ///
    /// Returns [`OAuthError::Setup`] when the TLS configuration cannot be built.
    pub(crate) fn new() -> Result<Self, OAuthError> {
        Self::build(reqwest::Client::builder())
    }

    /// Builds a client that ignores proxy settings, so tests reach their local server even on a
    /// Mac with a system proxy (the proxy settings have no loopback exception).
    #[cfg(test)]
    pub(crate) fn without_proxy() -> Result<Self, OAuthError> {
        Self::build(reqwest::Client::builder().no_proxy())
    }

    fn build(builder: reqwest::ClientBuilder) -> Result<Self, OAuthError> {
        let client = builder
            .timeout(TIMEOUT)
            .build()
            .map_err(OAuthError::Setup)?;
        Ok(Self { client })
    }

    /// Posts an `application/x-www-form-urlencoded` body and returns the status and body.
    ///
    /// # Arguments
    ///
    /// * `url` - Where to post.
    /// * `form` - The form fields.
    ///
    /// # Errors
    ///
    /// Returns [`OAuthError::Network`] when the request or the response body fails.
    pub(crate) async fn post_form(
        &self,
        url: &Url,
        form: &[(&str, &str)],
    ) -> Result<(u16, Vec<u8>), OAuthError> {
        let response = self
            .client
            .post(url.clone())
            .form(form)
            .send()
            .await
            .map_err(OAuthError::Network)?;
        let status = response.status().as_u16();
        let body = response.bytes().await.map_err(OAuthError::Network)?;
        Ok((status, body.to_vec()))
    }
}
