use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use clientmail_domain::ports::{AccountRepository, SecretStore};
use clientmail_domain::{
    AccessToken, Account, AccountId, NewAccount, OAuthIdentity, Provider, SecretKey, SecretKind,
    SecretString,
};

use crate::EngineError;

/// What the account use cases share: the account store, the secret store, one lock per account
/// and the cached access tokens.
///
/// Every use case holds it through an `Arc` (a shared, reference-counted pointer), so work that
/// runs on another thread can own a handle to it.
#[derive(Debug)]
pub struct AccountStores<R, S> {
    pub(crate) repository: R,
    pub(crate) secrets: S,
    pub(crate) locks: AccountLocks,
    pub(crate) tokens: TokenCache,
}

impl<R: AccountRepository, S: SecretStore> AccountStores<R, S> {
    /// Creates the shared state.
    ///
    /// # Arguments
    ///
    /// * `repository` - Where accounts are stored.
    /// * `secrets` - Where account secrets are stored.
    #[must_use]
    pub fn new(repository: R, secrets: S) -> Self {
        Self {
            repository,
            secrets,
            locks: AccountLocks::default(),
            tokens: TokenCache::default(),
        }
    }

    /// Stores the account Google signed in, with its refresh token and access token.
    ///
    /// A new address becomes a Gmail account: its row goes in first, then its token; if the token
    /// cannot be written, the row is deleted again, so no account is left without a token and no
    /// token without an account. An existing Gmail account gets the new token (signing in again),
    /// written under its lock only if it still exists; if it was removed after the lookup, the
    /// address becomes a new account instead.
    ///
    /// Two sign-ins of the same new address at once, or a manual add racing one, can both miss
    /// the lookup; the second insert then fails on the address's unique index and reports
    /// `Storage`. The UI allows one sign-in at a time, so only two windows can get here.
    pub(crate) fn save_signed_in(
        &self,
        identity: &OAuthIdentity,
        refresh: &SecretString,
        access: AccessToken,
    ) -> Result<Account, EngineError> {
        let existing = self
            .repository
            .find_by_address(identity.email())
            .map_err(EngineError::storage)?;
        match existing {
            Some(account) if account.provider() == Provider::Gmail => {
                if self.save_refreshed(account.id(), Some(refresh), access.clone())? {
                    return Ok(account);
                }
            }
            Some(_) => return Err(EngineError::DuplicateAccount),
            None => {}
        }
        let account = NewAccount::parse(identity.email().as_str(), identity.name())?
            .into_account_with(Provider::Gmail);
        self.locks.with(account.id(), || {
            self.insert_with_token(&account, refresh)?;
            self.tokens.put(account.id(), access);
            Ok::<_, EngineError>(())
        })?;
        Ok(account)
    }

    /// Stores new tokens of an existing account, if it still exists when its lock is held.
    ///
    /// # Returns
    ///
    /// `false` when the account was removed in the meantime; nothing is stored then.
    pub(crate) fn save_refreshed(
        &self,
        account: AccountId,
        refresh: Option<&SecretString>,
        access: AccessToken,
    ) -> Result<bool, EngineError> {
        self.locks.with(account, || {
            if !self
                .repository
                .exists(account)
                .map_err(EngineError::storage)?
            {
                return Ok(false);
            }
            if let Some(refresh) = refresh {
                self.write_token(account, refresh)?;
            }
            self.tokens.put(account, access);
            Ok(true)
        })
    }

    pub(crate) fn read_token(
        &self,
        account: AccountId,
    ) -> Result<Option<SecretString>, EngineError> {
        self.secrets
            .read(&token_key(account))
            .map_err(EngineError::secrets)
    }

    fn write_token(&self, account: AccountId, token: &SecretString) -> Result<(), EngineError> {
        self.secrets
            .write(&token_key(account), token)
            .map_err(EngineError::secrets)
    }

    fn insert_with_token(
        &self,
        account: &Account,
        token: &SecretString,
    ) -> Result<(), EngineError> {
        self.repository
            .insert(account)
            .map_err(EngineError::storage)?;
        if let Err(error) = self.write_token(account.id(), token) {
            // If this delete fails too, the storage error wins and the account stays without a
            // token; using it then asks the user to sign in again.
            self.repository
                .delete(account.id())
                .map_err(EngineError::storage)?;
            return Err(error);
        }
        Ok(())
    }
}

pub(crate) const fn token_key(account: AccountId) -> SecretKey {
    SecretKey::new(account, SecretKind::OAuthRefreshToken)
}

/// One lock per account, so writing an account's token and removing that account never overlap.
///
/// The locks are plain mutexes, taken only inside blocking work, never across an `.await`.
#[derive(Debug, Default)]
pub(crate) struct AccountLocks {
    locks: Mutex<HashMap<AccountId, Arc<Mutex<()>>>>,
}

impl AccountLocks {
    /// Runs `work` while holding the account's lock.
    pub(crate) fn with<T>(&self, account: AccountId, work: impl FnOnce() -> T) -> T {
        // A poisoned lock means a panic while it was held; `()` and the map stay consistent, so
        // the lock is still usable.
        let lock = Arc::clone(
            self.locks
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .entry(account)
                .or_default(),
        );
        let _guard = lock.lock().unwrap_or_else(PoisonError::into_inner);
        work()
    }

    /// Drops an account's lock after the account is removed.
    ///
    /// A writer still waiting on the old lock finds the account gone once it gets the lock; a new
    /// writer gets a new lock and finds the same.
    pub(crate) fn forget(&self, account: AccountId) {
        self.locks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&account);
    }
}

/// Access tokens, in memory only.
#[derive(Debug, Default)]
pub(crate) struct TokenCache {
    tokens: Mutex<HashMap<AccountId, AccessToken>>,
}

impl TokenCache {
    pub(crate) fn get(&self, account: AccountId) -> Option<AccessToken> {
        self.tokens
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&account)
            .cloned()
    }

    pub(crate) fn put(&self, account: AccountId, token: AccessToken) {
        self.tokens
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(account, token);
    }

    pub(crate) fn remove(&self, account: AccountId) {
        self.tokens
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&account);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    use clientmail_domain::{EmailAddress, UnixTime};

    use super::*;
    use crate::AccountService;
    use crate::fakes::{FakeStores, stores};

    const WAIT: Duration = Duration::from_secs(5);

    /// Holds `account`'s lock on another thread until the returned sender is used or dropped.
    fn hold_lock(
        stores: &Arc<FakeStores>,
        account: AccountId,
    ) -> (mpsc::Sender<()>, thread::JoinHandle<()>) {
        let (entered, inside) = mpsc::channel();
        let (release, gate) = mpsc::channel::<()>();
        let stores = Arc::clone(stores);
        let holder = thread::spawn(move || {
            stores.locks.with(account, || {
                entered.send(()).unwrap();
                // Released by a send or by the sender being dropped.
                let _released = gate.recv_timeout(WAIT);
            });
        });
        inside.recv_timeout(WAIT).unwrap();
        (release, holder)
    }

    /// Runs `work` on another thread and reports whether it finished while the lock was held,
    /// and whether it finished once the lock was released.
    fn runs_only_after_release(
        stores: &Arc<FakeStores>,
        account: AccountId,
        work: impl FnOnce() + Send + 'static,
    ) -> (bool, bool) {
        let (release, holder) = hold_lock(stores, account);
        let (done, finished) = mpsc::channel();
        let worker = thread::spawn(move || {
            work();
            done.send(()).unwrap();
        });
        let while_held = finished.recv_timeout(Duration::from_millis(100)).is_ok();
        release.send(()).unwrap();
        let after = finished.recv_timeout(WAIT).is_ok();
        holder.join().unwrap();
        worker.join().unwrap();
        (while_held, after)
    }

    #[test]
    fn removal_and_token_saves_wait_for_the_account_lock() {
        let stores = stores();
        let account = AccountService::new(Arc::clone(&stores))
            .add_account("ada@gmail.com", None)
            .unwrap()
            .id();
        let access = AccessToken::new(SecretString::from("access"), UnixTime::from_seconds(9));

        let save = {
            let saver = Arc::clone(&stores);
            runs_only_after_release(&stores, account, move || {
                saver.save_refreshed(account, None, access).unwrap();
            })
        };
        let removal = {
            let service = AccountService::new(Arc::clone(&stores));
            runs_only_after_release(&stores, account, move || {
                service.remove_account(account).unwrap();
            })
        };

        assert_eq!(save, (false, true), "a token save waits for the lock");
        assert_eq!(removal, (false, true), "a removal waits for the lock");
    }

    #[test]
    fn an_account_removed_after_the_lookup_is_added_again_without_an_orphan_token() {
        let stores = stores();
        let identity = OAuthIdentity::new(EmailAddress::parse("ada@gmail.com").unwrap(), None);
        let removed = NewAccount::parse("ada@gmail.com", None)
            .unwrap()
            .into_account_with(Provider::Gmail);
        *stores.repository.stale_lookup.lock().unwrap() = Some(removed.clone());

        let account = stores
            .save_signed_in(
                &identity,
                &SecretString::from("refresh"),
                AccessToken::new(SecretString::from("access"), UnixTime::from_seconds(9)),
            )
            .unwrap();

        assert_ne!(account.id(), removed.id(), "a new account");
        assert_eq!(
            stores.repository.accounts.lock().unwrap().clone(),
            vec![account.clone()],
            "its row exists"
        );
        assert!(
            stores.secrets.token(removed.id()).is_none(),
            "no orphan token"
        );
        assert!(
            stores.tokens.get(removed.id()).is_none(),
            "nothing cached for it"
        );
        assert_eq!(
            stores.secrets.token(account.id()).as_deref(),
            Some("refresh"),
            "the new account has the token"
        );
    }

    #[test]
    fn a_held_account_lock_blocks_the_same_account_only() {
        let locks = Arc::new(AccountLocks::default());
        let (held, other) = (AccountId::generate(), AccountId::generate());
        let (entered, inside) = mpsc::channel();
        let (release, gate) = mpsc::channel::<()>();
        let holder = {
            let locks = Arc::clone(&locks);
            thread::spawn(move || {
                locks.with(held, || {
                    entered.send(()).unwrap();
                    gate.recv_timeout(WAIT).unwrap();
                });
            })
        };
        inside.recv_timeout(WAIT).unwrap();
        let (same_done, same_finished) = mpsc::channel();
        let waiter = {
            let locks = Arc::clone(&locks);
            thread::spawn(move || locks.with(held, || same_done.send(()).unwrap()))
        };

        locks.with(other, || ());
        let same_ran_while_held = same_finished
            .recv_timeout(Duration::from_millis(100))
            .is_ok();
        release.send(()).unwrap();
        let same_ran_after = same_finished.recv_timeout(WAIT).is_ok();
        holder.join().unwrap();
        waiter.join().unwrap();

        assert!(!same_ran_while_held, "the same account waited");
        assert!(same_ran_after, "and ran once the lock was released");
    }
}
