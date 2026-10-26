use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};

use clientmail_domain::Account;
use clientmail_domain::ports::{AccountRepository, OAuthClient, SecretStore};
use clientmail_engine::{EngineError, GoogleSignIn, PendingSignIn};
use tokio::task::AbortHandle;

use crate::runtime::{CoreRuntime, RuntimeBlocking, finished};
use crate::{AccountRecord, CoreError};

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// The sign-in use case without its OAuth client's type, so the client and the session can hold
/// any `OAuthClient` (Google's in the app, a fake in tests) behind one pointer.
///
/// `OAuthClient` itself cannot be a trait object: its methods return `impl Future`.
pub(crate) trait SignInFlow: std::fmt::Debug + Send + Sync + 'static {
    fn start(&self) -> Result<PendingSignIn, EngineError>;
    fn scheme(&self) -> &str;
    fn finish(
        self: Arc<Self>,
        pending: PendingSignIn,
        callback_url: String,
    ) -> BoxFuture<Result<Account, EngineError>>;
}

impl<R, S, O> SignInFlow for GoogleSignIn<R, S, O, RuntimeBlocking>
where
    R: AccountRepository + std::fmt::Debug + Send + Sync + 'static,
    S: SecretStore + std::fmt::Debug + Send + Sync + 'static,
    O: OAuthClient + std::fmt::Debug,
{
    fn start(&self) -> Result<PendingSignIn, EngineError> {
        self.begin()
    }

    fn scheme(&self) -> &str {
        self.callback_scheme()
    }

    fn finish(
        self: Arc<Self>,
        pending: PendingSignIn,
        callback_url: String,
    ) -> BoxFuture<Result<Account, EngineError>> {
        Box::pin(async move { self.complete(pending, &callback_url).await })
    }
}

/// Where a session is in its single use.
enum Stage {
    Ready(PendingSignIn),
    Running(AbortHandle),
    Cancelled,
}

/// One Google sign-in, from the URL to open to the stored account. Used once.
///
/// Swift opens [`GoogleSignInSession::authorization_url`] in a browser sheet, then passes the
/// callback URL to [`GoogleSignInSession::complete`]. [`GoogleSignInSession::cancel`] stops it.
#[derive(uniffi::Object)]
pub struct GoogleSignInSession {
    url: String,
    scheme: String,
    flow: Arc<dyn SignInFlow>,
    // Keeps the runtime alive while the session exists, even if the client goes first.
    runtime: Arc<CoreRuntime>,
    stage: Mutex<Stage>,
}

impl GoogleSignInSession {
    pub(crate) fn start(
        flow: Arc<dyn SignInFlow>,
        runtime: Arc<CoreRuntime>,
    ) -> Result<Self, CoreError> {
        let pending = flow.start()?;
        Ok(Self {
            url: pending.url().to_owned(),
            scheme: flow.scheme().to_owned(),
            flow,
            runtime,
            stage: Mutex::new(Stage::Ready(pending)),
        })
    }

    fn stage(&self) -> std::sync::MutexGuard<'_, Stage> {
        // A panic while the lock was held leaves a valid `Stage`; keep using it.
        self.stage.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[uniffi::export]
impl GoogleSignInSession {
    /// Returns the URL to open in the browser sheet.
    #[must_use]
    pub fn authorization_url(&self) -> String {
        self.url.clone()
    }

    /// Returns the URL scheme the browser sheet waits for.
    #[must_use]
    pub fn callback_scheme(&self) -> String {
        self.scheme.clone()
    }

    /// Finishes the sign-in with the URL the browser was redirected to.
    ///
    /// The work runs on the core's workers, so [`GoogleSignInSession::cancel`] can stop it. Once
    /// the code exchange has finished, storing the account runs to the end even if cancelled; the
    /// account then appears on the next load.
    ///
    /// # Arguments
    ///
    /// * `callback_url` - The callback URL.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::SignInCancelled`] after a cancel, [`CoreError::SignInDenied`],
    /// [`CoreError::SignInFailed`] (also when called twice), [`CoreError::Network`],
    /// [`CoreError::DuplicateAccount`], or a storage or Keychain error.
    pub async fn complete(&self, callback_url: String) -> Result<AccountRecord, CoreError> {
        let task = {
            let mut stage = self.stage();
            let pending = match std::mem::replace(&mut *stage, Stage::Cancelled) {
                Stage::Ready(pending) => pending,
                Stage::Cancelled => return Err(CoreError::SignInCancelled),
                running @ Stage::Running(_) => {
                    *stage = running;
                    return Err(CoreError::SignInFailed);
                }
            };
            let task = self
                .runtime
                .spawn(Arc::clone(&self.flow).finish(pending, callback_url));
            *stage = Stage::Running(task.abort_handle());
            task
        };
        let account = finished(task.await).ok_or(CoreError::SignInCancelled)??;
        Ok(AccountRecord::from(&account))
    }

    /// Stops the sign-in. A waiting [`GoogleSignInSession::complete`] returns
    /// [`CoreError::SignInCancelled`]; a later one does too.
    pub fn cancel(&self) {
        let mut stage = self.stage();
        if let Stage::Running(task) = &*stage {
            task.abort();
        }
        *stage = Stage::Cancelled;
    }
}

impl std::fmt::Debug for GoogleSignInSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("GoogleSignInSession")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fmt;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;
    use std::time::{Duration, Instant};

    use clientmail_domain::{
        AccessToken, AuthorizationCode, AuthorizationRequest, EmailAddress, OAuthErrorKind,
        OAuthIdentity, OAuthState, PkceVerifier, SecretString, TokenGrant, UnixTime,
    };
    use clientmail_store::SqliteStore;
    use pollster::block_on;

    use super::*;
    use crate::{CoreClient, SecretBridgeError, SecretStoreBridge};

    #[derive(Debug)]
    struct TestError;

    impl fmt::Display for TestError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("test failure")
        }
    }

    impl std::error::Error for TestError {}

    /// OAuth client with one state, `s`. `park` makes the exchange wait forever, after setting
    /// `entered`.
    #[derive(Debug, Default)]
    struct TestOAuth {
        park: bool,
        entered: Arc<AtomicBool>,
    }

    impl OAuthClient for TestOAuth {
        type Error = TestError;

        fn error_kind(_error: &TestError) -> OAuthErrorKind {
            OAuthErrorKind::Protocol
        }

        fn authorization(&self) -> Result<AuthorizationRequest, TestError> {
            Ok(AuthorizationRequest::new(
                "https://auth.example/".to_owned(),
                OAuthState::new("s".to_owned()),
                PkceVerifier::new(SecretString::from("v".repeat(43))).unwrap(),
            ))
        }

        fn callback_scheme(&self) -> &'static str {
            "test"
        }

        fn parse_callback(
            &self,
            url: &str,
            expected: &OAuthState,
        ) -> Result<AuthorizationCode, TestError> {
            let state = url.strip_prefix("test:/cb?state=").ok_or(TestError)?;
            if state != expected.as_str() {
                return Err(TestError);
            }
            Ok(AuthorizationCode::new(SecretString::from("code")))
        }

        async fn exchange(
            &self,
            _code: AuthorizationCode,
            _verifier: PkceVerifier,
        ) -> Result<TokenGrant, TestError> {
            self.entered.store(true, Ordering::SeqCst);
            if self.park {
                std::future::pending::<()>().await;
            }
            Ok(TokenGrant::new(
                AccessToken::new(SecretString::from("access"), UnixTime::from_seconds(9_999)),
                Some(SecretString::from("refresh")),
                Some(OAuthIdentity::new(
                    EmailAddress::parse("ada@gmail.com").unwrap(),
                    Some("Ada".to_owned()),
                )),
                true,
            ))
        }

        fn refresh(
            &self,
            _refresh_token: SecretString,
        ) -> impl Future<Output = Result<TokenGrant, TestError>> + Send {
            std::future::ready(Err(TestError))
        }
    }

    #[derive(Default)]
    struct MemorySecrets {
        items: Mutex<HashMap<String, String>>,
    }

    impl SecretStoreBridge for MemorySecrets {
        fn read(&self, label: String) -> Result<Option<String>, SecretBridgeError> {
            Ok(self.items.lock().unwrap().get(&label).cloned())
        }

        fn write(&self, label: String, secret: String) -> Result<(), SecretBridgeError> {
            self.items.lock().unwrap().insert(label, secret);
            Ok(())
        }

        fn delete(&self, label: String) -> Result<bool, SecretBridgeError> {
            Ok(self.items.lock().unwrap().remove(&label).is_some())
        }
    }

    fn client(oauth: TestOAuth) -> (Arc<CoreClient>, Arc<MemorySecrets>) {
        let secrets = Arc::new(MemorySecrets::default());
        let client = CoreClient::with_parts(
            SqliteStore::open_in_memory().unwrap(),
            Arc::<MemorySecrets>::clone(&secrets),
            Some(Ok(oauth)),
        )
        .unwrap();
        (Arc::new(client), secrets)
    }

    #[test]
    fn a_client_that_could_not_be_built_shows_sign_in_and_fails_it() {
        let client = CoreClient::with_parts::<TestOAuth>(
            SqliteStore::open_in_memory().unwrap(),
            Arc::new(MemorySecrets::default()),
            Some(Err(clientmail_oauth::OAuthError::Random)),
        )
        .unwrap();

        assert!(
            client.google_sign_in_available(),
            "the button stays visible"
        );
        assert_eq!(
            client.begin_google_sign_in().err(),
            Some(CoreError::SignInFailed),
            "and reports the failure"
        );
    }

    #[test]
    fn a_completed_sign_in_stores_the_account_and_its_token() {
        let (client, secrets) = client(TestOAuth::default());
        let session = client.begin_google_sign_in().unwrap();

        let account = block_on(session.complete("test:/cb?state=s".to_owned())).unwrap();
        let listed = block_on(client.accounts()).unwrap();

        assert_eq!(listed, vec![account.clone()], "account listed");
        assert_eq!(
            secrets
                .items
                .lock()
                .unwrap()
                .get(&format!("{}:oauth-refresh-token", account.id))
                .map(String::as_str),
            Some("refresh"),
            "token in the bridge"
        );
    }

    #[test]
    fn a_session_completes_once() {
        let (client, _) = client(TestOAuth::default());
        let session = client.begin_google_sign_in().unwrap();
        block_on(session.complete("test:/cb?state=s".to_owned())).unwrap();

        let again = block_on(session.complete("test:/cb?state=s".to_owned()));

        assert_eq!(again, Err(CoreError::SignInFailed), "second call refused");
    }

    #[test]
    fn cancel_before_complete_stores_nothing() {
        let (client, _) = client(TestOAuth::default());
        let session = client.begin_google_sign_in().unwrap();

        session.cancel();
        let result = block_on(session.complete("test:/cb?state=s".to_owned()));

        assert_eq!(result, Err(CoreError::SignInCancelled), "cancelled");
        assert!(
            block_on(client.accounts()).unwrap().is_empty(),
            "nothing stored"
        );
    }

    #[test]
    fn cancel_during_the_exchange_returns_cancelled_and_stores_nothing() {
        let entered = Arc::new(AtomicBool::new(false));
        let (client, _) = client(TestOAuth {
            park: true,
            entered: Arc::clone(&entered),
        });
        let session = client.begin_google_sign_in().unwrap();
        let waiting = {
            let session = Arc::clone(&session);
            thread::spawn(move || block_on(session.complete("test:/cb?state=s".to_owned())))
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        while !entered.load(Ordering::SeqCst) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }

        session.cancel();
        let result = waiting.join().unwrap();

        assert!(entered.load(Ordering::SeqCst), "the exchange was running");
        assert_eq!(result, Err(CoreError::SignInCancelled), "cancelled");
        assert!(
            block_on(client.accounts()).unwrap().is_empty(),
            "nothing stored"
        );
    }

    #[test]
    fn a_session_outlives_its_client() {
        let (client, _) = client(TestOAuth::default());
        let session = client.begin_google_sign_in().unwrap();
        drop(client);

        let result = block_on(session.complete("test:/cb?state=s".to_owned()));

        assert!(result.is_ok(), "the session keeps the runtime: {result:?}");
    }
}
