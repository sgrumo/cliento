use std::sync::Arc;

use clientmail_domain::ports::{AccountRepository, Blocking, OAuthClient, SecretStore};
use clientmail_domain::{Account, AuthorizationRequest};

use crate::{AccountStores, EngineError, SignInProblem};

/// Adds a Gmail account by signing in with Google (OAuth 2.0 with PKCE).
///
/// Generic over the OAuth protocol `O` and how blocking work runs `B`, so tests pass fakes. The
/// account store and the secret store are synchronous; every call to them goes through
/// [`Blocking::run`] (decision record 0009).
#[derive(Debug)]
pub struct GoogleSignIn<R, S, O, B> {
    stores: Arc<AccountStores<R, S>>,
    oauth: Arc<O>,
    blocking: B,
}

/// A sign-in that has started: the URL to open, and what the callback is checked against. It
/// holds the PKCE verifier, so it is used once.
#[derive(Debug)]
pub struct PendingSignIn {
    request: AuthorizationRequest,
}

impl PendingSignIn {
    /// Returns the URL to open in the browser sheet.
    #[must_use]
    pub fn url(&self) -> &str {
        self.request.url()
    }
}

impl<R, S, O, B> GoogleSignIn<R, S, O, B>
where
    R: AccountRepository + Send + Sync + 'static,
    S: SecretStore + Send + Sync + 'static,
    O: OAuthClient,
    B: Blocking,
{
    /// Creates the use case.
    ///
    /// # Arguments
    ///
    /// * `stores` - The state shared with the other account use cases.
    /// * `oauth` - Google's OAuth protocol.
    /// * `blocking` - Where synchronous store calls run.
    #[must_use]
    pub const fn new(stores: Arc<AccountStores<R, S>>, oauth: Arc<O>, blocking: B) -> Self {
        Self {
            stores,
            oauth,
            blocking,
        }
    }

    /// Returns the URL scheme the browser sheet waits for.
    #[must_use]
    pub fn callback_scheme(&self) -> &str {
        self.oauth.callback_scheme()
    }

    /// Starts a sign-in: a fresh PKCE verifier and state. Nothing is stored.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError::SignInFailed`] when the provider client cannot start one (no
    /// random bytes).
    pub fn begin(&self) -> Result<PendingSignIn, EngineError> {
        let request = self
            .oauth
            .authorization()
            .map_err(|error| EngineError::oauth::<O>(error, false))?;
        Ok(PendingSignIn { request })
    }

    /// Finishes a sign-in with the URL the browser was redirected to.
    ///
    /// Checks the callback against the pending sign-in, exchanges the code, and stores the
    /// account and its refresh token (see [`AccountStores`]). Signing in again with an existing
    /// Gmail account replaces its token.
    ///
    /// # Arguments
    ///
    /// * `pending` - What [`GoogleSignIn::begin`] returned.
    /// * `callback_url` - The callback URL.
    ///
    /// # Returns
    ///
    /// The new or existing account.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError::SignInDenied`] when the user refused consent,
    /// [`EngineError::MailAccessNotGranted`] when they signed in but refused mail access,
    /// [`EngineError::SignInFailed`] for a callback of another sign-in, an incomplete grant or a
    /// provider error, [`EngineError::Network`] when Google cannot be reached,
    /// [`EngineError::DuplicateAccount`] when a non-Gmail account has the address, and
    /// [`EngineError::Storage`] or [`EngineError::Secrets`] when storing fails.
    pub async fn complete(
        &self,
        pending: PendingSignIn,
        callback_url: &str,
    ) -> Result<Account, EngineError> {
        let (_, state, verifier) = pending.request.into_parts();
        // The port checks the callback's state before anything else (RFC 6749, section 10.12).
        let code = self
            .oauth
            .parse_callback(callback_url, &state)
            .map_err(|error| EngineError::oauth::<O>(error, false))?;
        let grant = self
            .oauth
            .exchange(code, verifier)
            .await
            .map_err(|error| EngineError::oauth::<O>(error, false))?;
        if !grant.mail_access() {
            return Err(EngineError::MailAccessNotGranted);
        }
        let (access, refresh, identity) = grant.into_parts();
        let refresh = refresh.ok_or_else(|| problem(SignInProblem::NoRefreshToken))?;
        let identity = identity.ok_or_else(|| problem(SignInProblem::NoIdentity))?;
        let stores = Arc::clone(&self.stores);
        self.blocking
            .run(move || stores.save_signed_in(&identity, &refresh, access))
            .await
    }
}

fn problem(problem: SignInProblem) -> EngineError {
    EngineError::SignInFailed(Box::new(problem))
}

#[cfg(test)]
mod tests {
    use clientmail_domain::{OAuthErrorKind, Provider};
    use pollster::block_on;

    use super::*;
    use crate::AccountService;
    use crate::fakes::{
        Answer, FakeOAuth, FakeRepository, FakeSecrets, FakeStores, InlineBlocking, stores,
    };

    type SignIn = GoogleSignIn<FakeRepository, FakeSecrets, FakeOAuth, InlineBlocking>;

    const ADA: (&str, Option<&str>) = ("ada@gmail.com", Some("Ada Lovelace"));

    fn granted(identity: Option<(&'static str, Option<&'static str>)>) -> Answer {
        Answer::Grant {
            access: "access",
            expires_at: 9_999,
            refresh: Some("refresh"),
            identity,
        }
    }

    fn sign_in(stores: &Arc<FakeStores>, answers: impl IntoIterator<Item = Answer>) -> SignIn {
        GoogleSignIn::new(
            Arc::clone(stores),
            FakeOAuth::answering(answers),
            InlineBlocking,
        )
    }

    /// Begins a sign-in and completes it with a callback carrying the right state.
    fn run(sign_in: &SignIn) -> Result<Account, EngineError> {
        let pending = sign_in.begin().unwrap();
        let state = pending.request.state().as_str().to_owned();
        block_on(sign_in.complete(pending, &format!("fake:/cb?code=c&state={state}")))
    }

    #[test]
    fn a_new_address_becomes_a_gmail_account_with_its_token() {
        let stores = stores();
        let sign_in = sign_in(
            &stores,
            [granted(Some(("ada@workspace.example", Some("Ada"))))],
        );

        let account = run(&sign_in).unwrap();

        assert_eq!(
            account.provider(),
            Provider::Gmail,
            "Gmail even on Workspace"
        );
        assert_eq!(account.display_name(), Some("Ada"), "Google's name");
        assert_eq!(
            stores.secrets.token(account.id()).as_deref(),
            Some("refresh"),
            "refresh token stored"
        );
        assert!(
            stores.tokens.get(account.id()).is_some(),
            "access token cached"
        );
    }

    #[test]
    fn signing_in_again_replaces_the_token_of_the_same_account() {
        let stores = stores();
        let first = run(&sign_in(&stores, [granted(Some(ADA))])).unwrap();
        let again = sign_in(
            &stores,
            [Answer::Grant {
                access: "access-2",
                expires_at: 9_999,
                refresh: Some("refresh-2"),
                identity: Some(ADA),
            }],
        );

        let second = run(&again).unwrap();

        assert_eq!(second.id(), first.id(), "same account");
        assert_eq!(
            stores.repository.accounts.lock().unwrap().len(),
            1,
            "no new row"
        );
        assert_eq!(
            stores.secrets.token(first.id()).as_deref(),
            Some("refresh-2"),
            "token replaced"
        );
    }

    #[test]
    fn a_non_gmail_account_with_the_address_is_a_duplicate() {
        let stores = stores();
        AccountService::new(Arc::clone(&stores))
            .add_account("ada@example.com", None)
            .unwrap();

        let result = run(&sign_in(
            &stores,
            [granted(Some(("ada@example.com", None)))],
        ));

        assert!(
            matches!(result, Err(EngineError::DuplicateAccount)),
            "{result:?}"
        );
    }

    #[test]
    fn a_failed_token_write_leaves_no_account() {
        let stores = Arc::new(AccountStores::new(
            FakeRepository::default(),
            FakeSecrets::failing_writes(),
        ));

        let result = run(&sign_in(&stores, [granted(Some(ADA))]));

        assert!(matches!(result, Err(EngineError::Secrets(_))), "{result:?}");
        assert!(
            stores.repository.accounts.lock().unwrap().is_empty(),
            "row removed again"
        );
    }

    #[test]
    fn a_callback_of_another_sign_in_is_refused_before_the_exchange() {
        let stores = stores();
        let oauth = FakeOAuth::answering([granted(Some(ADA))]);
        let sign_in = GoogleSignIn::new(Arc::clone(&stores), Arc::clone(&oauth), InlineBlocking);
        let pending = sign_in.begin().unwrap();

        let result = block_on(sign_in.complete(pending, "fake:/cb?code=c&state=forged"));

        assert!(
            matches!(result, Err(EngineError::SignInFailed(_))),
            "{result:?}"
        );
        assert_eq!(*oauth.exchanges.lock().unwrap(), 0, "no exchange");
    }

    #[test]
    fn a_grant_without_mail_access_stores_nothing() {
        let stores = stores();

        let result = run(&sign_in(&stores, [Answer::WithoutMailAccess]));

        assert!(
            matches!(result, Err(EngineError::MailAccessNotGranted)),
            "{result:?}"
        );
        assert!(
            stores.repository.accounts.lock().unwrap().is_empty(),
            "nothing stored"
        );
    }

    #[test]
    fn an_incomplete_grant_fails_and_stores_nothing() {
        for (answer, expected) in [
            (granted(None), SignInProblem::NoIdentity),
            (
                Answer::Grant {
                    access: "a",
                    expires_at: 1,
                    refresh: None,
                    identity: Some(ADA),
                },
                SignInProblem::NoRefreshToken,
            ),
        ] {
            let stores = stores();

            let result = run(&sign_in(&stores, [answer]));

            assert!(
                matches!(&result, Err(EngineError::SignInFailed(source))
                    if source.downcast_ref() == Some(&expected)),
                "{result:?}"
            );
            assert!(
                stores.repository.accounts.lock().unwrap().is_empty(),
                "nothing stored"
            );
        }
    }

    #[test]
    fn provider_errors_are_classified() {
        let denied = {
            let sign_in = sign_in(&stores(), []);
            let pending = sign_in.begin().unwrap();
            block_on(sign_in.complete(pending, "fake:/cb?error=denied&state=state-1"))
        };
        let offline = run(&sign_in(&stores(), [Answer::Fail(OAuthErrorKind::Network)]));
        let rejected = run(&sign_in(
            &stores(),
            [Answer::Fail(OAuthErrorKind::Rejected)],
        ));

        assert!(
            matches!(denied, Err(EngineError::SignInDenied)),
            "{denied:?}"
        );
        assert!(
            matches!(offline, Err(EngineError::Network(_))),
            "{offline:?}"
        );
        assert!(
            matches!(rejected, Err(EngineError::SignInFailed(_))),
            "a refused code fails the sign-in: {rejected:?}"
        );
    }
}
