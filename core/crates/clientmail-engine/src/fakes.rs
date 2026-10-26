//! Thread-safe fakes of every port the engine uses, shared by the use case tests.

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use clientmail_domain::ports::{AccountRepository, Blocking, Clock, OAuthClient, SecretStore};
use clientmail_domain::{
    AccessToken, Account, AccountId, AuthorizationCode, AuthorizationRequest, EmailAddress,
    ExposeSecret, OAuthErrorKind, OAuthIdentity, OAuthState, PkceVerifier, SecretKey, SecretString,
    TokenGrant, UnixTime,
};

use crate::AccountStores;
use crate::stores::token_key;

#[derive(Debug)]
pub(crate) struct FakeError;

impl fmt::Display for FakeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("fake failure")
    }
}

impl std::error::Error for FakeError {}

/// In-memory repository. `stale_lookup`, when set, is what `find_by_address` returns, to stage
/// an account removed between a lookup and a later write.
#[derive(Default)]
pub(crate) struct FakeRepository {
    pub(crate) accounts: Mutex<Vec<Account>>,
    pub(crate) stale_lookup: Mutex<Option<Account>>,
}

impl AccountRepository for FakeRepository {
    type Error = FakeError;

    fn list(&self) -> Result<Vec<Account>, FakeError> {
        Ok(self.accounts.lock().unwrap().clone())
    }

    fn find_by_address(&self, address: &EmailAddress) -> Result<Option<Account>, FakeError> {
        let stale = self.stale_lookup.lock().unwrap().take();
        if let Some(stale) = stale {
            return Ok(Some(stale));
        }
        Ok(self
            .accounts
            .lock()
            .unwrap()
            .iter()
            .find(|account| account.address() == address)
            .cloned())
    }

    fn insert(&self, account: &Account) -> Result<(), FakeError> {
        self.accounts.lock().unwrap().push(account.clone());
        Ok(())
    }

    fn exists(&self, id: AccountId) -> Result<bool, FakeError> {
        Ok(self
            .accounts
            .lock()
            .unwrap()
            .iter()
            .any(|account| account.id() == id))
    }

    fn delete(&self, id: AccountId) -> Result<bool, FakeError> {
        let mut accounts = self.accounts.lock().unwrap();
        let before = accounts.len();
        accounts.retain(|account| account.id() != id);
        Ok(accounts.len() < before)
    }
}

/// Repository whose every call fails.
pub(crate) struct FailingRepository;

impl AccountRepository for FailingRepository {
    type Error = FakeError;

    fn list(&self) -> Result<Vec<Account>, FakeError> {
        Err(FakeError)
    }

    fn find_by_address(&self, _address: &EmailAddress) -> Result<Option<Account>, FakeError> {
        Err(FakeError)
    }

    fn insert(&self, _account: &Account) -> Result<(), FakeError> {
        Err(FakeError)
    }

    fn exists(&self, _id: AccountId) -> Result<bool, FakeError> {
        Err(FakeError)
    }

    fn delete(&self, _id: AccountId) -> Result<bool, FakeError> {
        Err(FakeError)
    }
}

/// In-memory secret store keyed by label. `failing` fails every call; `failing_writes` only
/// writes.
#[derive(Default)]
pub(crate) struct FakeSecrets {
    pub(crate) items: Mutex<HashMap<String, String>>,
    pub(crate) failing: bool,
    pub(crate) failing_writes: bool,
}

impl FakeSecrets {
    pub(crate) fn failing() -> Self {
        Self {
            failing: true,
            ..Self::default()
        }
    }

    pub(crate) fn failing_writes() -> Self {
        Self {
            failing_writes: true,
            ..Self::default()
        }
    }

    /// Returns the stored refresh token of an account, as plain text.
    pub(crate) fn token(&self, account: AccountId) -> Option<String> {
        self.items
            .lock()
            .unwrap()
            .get(&token_key(account).label())
            .cloned()
    }

    fn check(&self) -> Result<(), FakeError> {
        if self.failing { Err(FakeError) } else { Ok(()) }
    }
}

impl SecretStore for FakeSecrets {
    type Error = FakeError;

    fn read(&self, key: &SecretKey) -> Result<Option<SecretString>, FakeError> {
        self.check()?;
        Ok(self
            .items
            .lock()
            .unwrap()
            .get(&key.label())
            .map(|value| SecretString::from(value.clone())))
    }

    fn write(&self, key: &SecretKey, secret: &SecretString) -> Result<(), FakeError> {
        self.check()?;
        if self.failing_writes {
            return Err(FakeError);
        }
        self.items
            .lock()
            .unwrap()
            .insert(key.label(), secret.expose_secret().to_owned());
        Ok(())
    }

    fn delete(&self, key: &SecretKey) -> Result<bool, FakeError> {
        self.check()?;
        Ok(self.items.lock().unwrap().remove(&key.label()).is_some())
    }
}

pub(crate) type FakeStores = AccountStores<FakeRepository, FakeSecrets>;

pub(crate) fn stores() -> Arc<FakeStores> {
    Arc::new(AccountStores::new(
        FakeRepository::default(),
        FakeSecrets::default(),
    ))
}

/// Runs blocking work at once, on the calling thread.
#[derive(Debug, Clone, Copy)]
pub(crate) struct InlineBlocking;

impl Blocking for InlineBlocking {
    fn run<T, F>(&self, work: F) -> impl Future<Output = T> + Send + 'static
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        std::future::ready(work())
    }
}

/// A clock the test sets. Clones share the same time.
#[derive(Debug, Clone, Default)]
pub(crate) struct FakeClock(Arc<AtomicI64>);

impl FakeClock {
    pub(crate) fn set(&self, seconds: i64) {
        self.0.store(seconds, Ordering::SeqCst);
    }
}

impl Clock for FakeClock {
    fn now(&self) -> UnixTime {
        UnixTime::from_seconds(self.0.load(Ordering::SeqCst))
    }
}

/// The error of [`FakeOAuth`], carrying the kind it reports.
#[derive(Debug)]
pub(crate) struct FakeOAuthError(pub(crate) OAuthErrorKind);

impl fmt::Display for FakeOAuthError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "fake OAuth failure: {:?}", self.0)
    }
}

impl std::error::Error for FakeOAuthError {}

/// What [`FakeOAuth`] returns from a token call.
pub(crate) enum Answer {
    Grant {
        access: &'static str,
        expires_at: i64,
        refresh: Option<&'static str>,
        identity: Option<(&'static str, Option<&'static str>)>,
    },
    /// A sign-in grant for ada@gmail.com in which the user refused mail access.
    WithoutMailAccess,
    Fail(OAuthErrorKind),
}

/// Scripted OAuth provider: hands out `state-<n>` states, answers token calls in order, and
/// records how often each was made.
#[derive(Default)]
pub(crate) struct FakeOAuth {
    answers: Mutex<VecDeque<Answer>>,
    issued: Mutex<u32>,
    pub(crate) exchanges: Mutex<u32>,
    pub(crate) refreshes: Mutex<u32>,
    /// Runs inside `refresh`, after the answer is chosen, to stage races.
    pub(crate) on_refresh: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

impl FakeOAuth {
    pub(crate) fn answering(answers: impl IntoIterator<Item = Answer>) -> Arc<Self> {
        Arc::new(Self {
            answers: Mutex::new(answers.into_iter().collect()),
            ..Self::default()
        })
    }

    fn next(&self) -> Result<TokenGrant, FakeOAuthError> {
        let answer = self
            .answers
            .lock()
            .unwrap()
            .pop_front()
            .expect("a scripted answer");
        match answer {
            Answer::Grant {
                access,
                expires_at,
                refresh,
                identity,
            } => Ok(TokenGrant::new(
                AccessToken::new(
                    SecretString::from(access),
                    UnixTime::from_seconds(expires_at),
                ),
                refresh.map(SecretString::from),
                identity.map(|(email, name)| {
                    OAuthIdentity::new(EmailAddress::parse(email).unwrap(), name.map(Into::into))
                }),
                true,
            )),
            Answer::WithoutMailAccess => Ok(TokenGrant::new(
                AccessToken::new(SecretString::from("access"), UnixTime::from_seconds(9_999)),
                Some(SecretString::from("refresh")),
                Some(OAuthIdentity::new(
                    EmailAddress::parse("ada@gmail.com").unwrap(),
                    None,
                )),
                false,
            )),
            Answer::Fail(kind) => Err(FakeOAuthError(kind)),
        }
    }
}

impl OAuthClient for FakeOAuth {
    type Error = FakeOAuthError;

    fn error_kind(error: &FakeOAuthError) -> OAuthErrorKind {
        error.0
    }

    fn authorization(&self) -> Result<AuthorizationRequest, FakeOAuthError> {
        let issued = {
            let mut issued = self.issued.lock().unwrap();
            *issued += 1;
            *issued
        };
        Ok(AuthorizationRequest::new(
            format!("https://auth.example/?n={issued}"),
            OAuthState::new(format!("state-{issued}")),
            PkceVerifier::new(SecretString::from("v".repeat(43))).unwrap(),
        ))
    }

    fn callback_scheme(&self) -> &'static str {
        "fake"
    }

    /// `fake:/cb?code=<code>&state=<state>`, or `fake:/cb?error=denied&state=<state>`. The state
    /// must match, as the real client checks.
    fn parse_callback(
        &self,
        url: &str,
        expected: &OAuthState,
    ) -> Result<AuthorizationCode, FakeOAuthError> {
        let query = url
            .strip_prefix("fake:/cb?")
            .ok_or(FakeOAuthError(OAuthErrorKind::Protocol))?;
        let (first, state) = query
            .split_once("&state=")
            .ok_or(FakeOAuthError(OAuthErrorKind::Protocol))?;
        if state != expected.as_str() {
            return Err(FakeOAuthError(OAuthErrorKind::Protocol));
        }
        if first == "error=denied" {
            return Err(FakeOAuthError(OAuthErrorKind::Denied));
        }
        let code = first
            .strip_prefix("code=")
            .ok_or(FakeOAuthError(OAuthErrorKind::Protocol))?;
        Ok(AuthorizationCode::new(SecretString::from(code)))
    }

    fn exchange(
        &self,
        _code: AuthorizationCode,
        _verifier: PkceVerifier,
    ) -> impl Future<Output = Result<TokenGrant, FakeOAuthError>> + Send {
        *self.exchanges.lock().unwrap() += 1;
        std::future::ready(self.next())
    }

    /// Runs `on_refresh` after choosing the answer, as if the removal happened while the request
    /// was on the network.
    fn refresh(
        &self,
        _refresh_token: SecretString,
    ) -> impl Future<Output = Result<TokenGrant, FakeOAuthError>> + Send {
        *self.refreshes.lock().unwrap() += 1;
        let answer = self.next();
        let hook = self.on_refresh.lock().unwrap().take();
        if let Some(hook) = hook {
            hook();
        }
        std::future::ready(answer)
    }
}
