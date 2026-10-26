use std::sync::Arc;

use clientmail_domain::ports::{AccountRepository, Blocking, Clock, OAuthClient, SecretStore};
use clientmail_domain::{AccountId, SecretString};

use crate::{AccountStores, EngineError};

/// An access token is refreshed when it has less than this many seconds left, so a request that
/// starts with it does not see it expire midway.
const EXPIRY_MARGIN_SECONDS: i64 = 60;

/// Hands out access tokens for signed-in accounts, refreshing them when needed.
///
/// Access tokens stay in memory. The refresh token is read from the secret store through
/// [`Blocking::run`]; a new one from the provider replaces it, under the account's lock and only
/// if the account still exists.
///
/// Two refreshes of one account at the same time both reach the provider and both get valid
/// tokens; refreshes happen about once an hour, so this is not deduplicated.
#[derive(Debug)]
pub struct TokenService<R, S, O, B, C> {
    stores: Arc<AccountStores<R, S>>,
    oauth: Arc<O>,
    blocking: B,
    clock: C,
}

impl<R, S, O, B, C> TokenService<R, S, O, B, C>
where
    R: AccountRepository + Send + Sync + 'static,
    S: SecretStore + Send + Sync + 'static,
    O: OAuthClient,
    B: Blocking,
    C: Clock,
{
    /// Creates the use case.
    ///
    /// # Arguments
    ///
    /// * `stores` - The state shared with the other account use cases.
    /// * `oauth` - The provider's OAuth protocol.
    /// * `blocking` - Where synchronous store calls run.
    /// * `clock` - The current time, for token expiry.
    #[must_use]
    pub const fn new(
        stores: Arc<AccountStores<R, S>>,
        oauth: Arc<O>,
        blocking: B,
        clock: C,
    ) -> Self {
        Self {
            stores,
            oauth,
            blocking,
            clock,
        }
    }

    /// Returns an access token for an account, valid for at least another minute.
    ///
    /// # Arguments
    ///
    /// * `account` - The account.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError::SignInRequired`] when no refresh token is stored or the provider
    /// refused it, [`EngineError::AccountNotFound`] when the account was removed during the
    /// refresh, [`EngineError::Network`] when the provider cannot be reached,
    /// [`EngineError::SignInFailed`] for other provider errors, and [`EngineError::Secrets`] or
    /// [`EngineError::Storage`] when the stores fail.
    pub async fn access_token(&self, account: AccountId) -> Result<SecretString, EngineError> {
        let now = self.clock.now();
        if let Some(token) = self.stores.tokens.get(account)
            && token.is_valid_for(now, EXPIRY_MARGIN_SECONDS)
        {
            return Ok(token.secret().clone());
        }
        let stores = Arc::clone(&self.stores);
        let refresh = self
            .blocking
            .run(move || stores.read_token(account))
            .await?
            .ok_or(EngineError::SignInRequired)?;
        let grant = self
            .oauth
            .refresh(refresh)
            .await
            .map_err(|error| EngineError::oauth::<O>(error, true))?;
        let (access, new_refresh, _) = grant.into_parts();
        let token = access.secret().clone();
        let stores = Arc::clone(&self.stores);
        let saved = self
            .blocking
            .run(move || stores.save_refreshed(account, new_refresh.as_ref(), access))
            .await?;
        if saved {
            Ok(token)
        } else {
            Err(EngineError::AccountNotFound)
        }
    }
}

#[cfg(test)]
mod tests {
    use clientmail_domain::{AccessToken, ExposeSecret, OAuthErrorKind, UnixTime};
    use pollster::block_on;

    use super::*;
    use crate::AccountService;
    use crate::fakes::{
        Answer, FakeClock, FakeOAuth, FakeRepository, FakeSecrets, FakeStores, InlineBlocking,
        stores,
    };
    use crate::stores::token_key;

    type Tokens = TokenService<FakeRepository, FakeSecrets, FakeOAuth, InlineBlocking, FakeClock>;

    struct Setup {
        stores: Arc<FakeStores>,
        oauth: Arc<FakeOAuth>,
        clock: FakeClock,
        tokens: Tokens,
        account: AccountId,
    }

    /// An account with a stored refresh token, a clock at 1 000 s, and scripted refreshes.
    fn setup(answers: impl IntoIterator<Item = Answer>) -> Setup {
        let stores = stores();
        let account = AccountService::new(Arc::clone(&stores))
            .add_account("ada@gmail.com", None)
            .unwrap()
            .id();
        stores
            .secrets
            .write(&token_key(account), &SecretString::from("refresh"))
            .unwrap();
        let oauth = FakeOAuth::answering(answers);
        let clock = FakeClock::default();
        clock.set(1_000);
        let tokens = TokenService::new(
            Arc::clone(&stores),
            Arc::clone(&oauth),
            InlineBlocking,
            clock.clone(),
        );
        Setup {
            stores,
            oauth,
            clock,
            tokens,
            account,
        }
    }

    fn refreshed(access: &'static str, refresh: Option<&'static str>) -> Answer {
        Answer::Grant {
            access,
            expires_at: 4_600,
            refresh,
            identity: None,
        }
    }

    fn token(setup: &Setup) -> Result<String, EngineError> {
        block_on(setup.tokens.access_token(setup.account))
            .map(|token| token.expose_secret().to_owned())
    }

    #[test]
    fn a_cached_token_is_reused_until_a_minute_before_expiry() {
        let setup = setup([refreshed("second", None)]);
        setup.stores.tokens.put(
            setup.account,
            AccessToken::new(SecretString::from("first"), UnixTime::from_seconds(2_000)),
        );

        let early = token(&setup).unwrap();
        setup.clock.set(1_940);
        let late = token(&setup).unwrap();

        assert_eq!(early, "first", "cached");
        assert_eq!(late, "second", "refreshed 60 s before expiry");
        assert_eq!(*setup.oauth.refreshes.lock().unwrap(), 1, "one refresh");
    }

    #[test]
    fn a_refresh_is_cached_and_a_rotated_refresh_token_is_stored() {
        let setup = setup([refreshed("access", Some("rotated"))]);

        let first = token(&setup).unwrap();
        let second = token(&setup).unwrap();

        assert_eq!(
            (first.as_str(), second.as_str()),
            ("access", "access"),
            "cached"
        );
        assert_eq!(*setup.oauth.refreshes.lock().unwrap(), 1, "refreshed once");
        assert_eq!(
            setup.stores.secrets.token(setup.account).as_deref(),
            Some("rotated"),
            "new refresh token stored"
        );
    }

    #[test]
    fn a_refused_or_missing_refresh_token_requires_signing_in_again() {
        let refused = setup([Answer::Fail(OAuthErrorKind::Rejected)]);
        let missing = setup([]);
        missing
            .stores
            .secrets
            .delete(&token_key(missing.account))
            .unwrap();

        let refused = token(&refused);
        let missing = token(&missing);

        assert!(
            matches!(refused, Err(EngineError::SignInRequired)),
            "{refused:?}"
        );
        assert!(
            matches!(missing, Err(EngineError::SignInRequired)),
            "{missing:?}"
        );
    }

    #[test]
    fn network_failures_pass_through() {
        let setup = setup([Answer::Fail(OAuthErrorKind::Network)]);

        let result = token(&setup);

        assert!(matches!(result, Err(EngineError::Network(_))), "{result:?}");
    }

    #[test]
    fn a_refresh_that_finishes_after_removal_stores_nothing() {
        let setup = setup([refreshed("access", Some("rotated"))]);
        let service = AccountService::new(Arc::clone(&setup.stores));
        let account = setup.account;
        *setup.oauth.on_refresh.lock().unwrap() = Some(Box::new(move || {
            service.remove_account(account).unwrap();
        }));

        let result = token(&setup);

        assert!(
            matches!(result, Err(EngineError::AccountNotFound)),
            "{result:?}"
        );
        assert!(
            setup.stores.secrets.token(account).is_none(),
            "no token written"
        );
        assert!(setup.stores.tokens.get(account).is_none(), "nothing cached");
    }
}
