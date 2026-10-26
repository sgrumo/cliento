use std::sync::Arc;

use clientmail_domain::ports::{AccountRepository, SecretStore};
use clientmail_domain::{Account, AccountId, NewAccount};

use crate::{AccountStores, EngineError};

/// Adds, lists and removes accounts.
///
/// Generic over the repository and the secret store so tests can pass fakes. `R` and `S` are
/// resolved at compile time, so this costs nothing at runtime compared to concrete types. Its
/// methods block on storage; callers on an async runtime run them on a blocking thread.
#[derive(Debug)]
pub struct AccountService<R, S> {
    stores: Arc<AccountStores<R, S>>,
}

impl<R: AccountRepository, S: SecretStore> AccountService<R, S> {
    /// Creates the service.
    ///
    /// # Arguments
    ///
    /// * `stores` - The state shared with the other account use cases.
    #[must_use]
    pub const fn new(stores: Arc<AccountStores<R, S>>) -> Self {
        Self { stores }
    }

    /// Returns every account, oldest first.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError::Storage`] when the repository fails.
    pub fn accounts(&self) -> Result<Vec<Account>, EngineError> {
        self.stores.repository.list().map_err(EngineError::storage)
    }

    /// Validates and stores a new account.
    ///
    /// # Arguments
    ///
    /// * `address` - The email address as typed.
    /// * `display_name` - The sender name, if any.
    ///
    /// # Returns
    ///
    /// The stored account, with its id and detected provider.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError::InvalidInput`] for an invalid address,
    /// [`EngineError::DuplicateAccount`] when the address is already configured, and
    /// [`EngineError::Storage`] when the repository fails.
    pub fn add_account(
        &self,
        address: &str,
        display_name: Option<&str>,
    ) -> Result<Account, EngineError> {
        let request = NewAccount::parse(address, display_name)?;
        let repository = &self.stores.repository;
        let existing = repository
            .find_by_address(request.address())
            .map_err(EngineError::storage)?;
        if existing.is_some() {
            return Err(EngineError::DuplicateAccount);
        }
        let account = request.into_account();
        repository.insert(&account).map_err(EngineError::storage)?;
        Ok(account)
    }

    /// Removes an account, its secrets and its cached access token.
    ///
    /// The account must exist; then, under the account's lock, its secrets go first, so that if
    /// they cannot be deleted the account stays and the user can retry, instead of a token
    /// outliving the account. The lock keeps a token refresh from writing in between.
    ///
    /// # Arguments
    ///
    /// * `id` - The account to remove.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError::AccountNotFound`] when no account has this id,
    /// [`EngineError::Secrets`] when the secrets cannot be deleted (the account row is kept, and
    /// some of its secrets may already be gone), and [`EngineError::Storage`] when the repository
    /// fails.
    pub fn remove_account(&self, id: AccountId) -> Result<(), EngineError> {
        let stores = &self.stores;
        if !stores.repository.exists(id).map_err(EngineError::storage)? {
            return Err(EngineError::AccountNotFound);
        }
        let deleted = stores.locks.with(id, || {
            stores
                .secrets
                .delete_account(id)
                .map_err(EngineError::secrets)?;
            let deleted = stores.repository.delete(id).map_err(EngineError::storage)?;
            stores.tokens.remove(id);
            Ok::<_, EngineError>(deleted)
        })?;
        stores.locks.forget(id);
        if deleted {
            Ok(())
        } else {
            Err(EngineError::AccountNotFound)
        }
    }
}

#[cfg(test)]
mod tests {
    use clientmail_domain::{AccessToken, DomainError, Provider, SecretString, UnixTime};

    use super::*;
    use crate::fakes::{FailingRepository, FakeRepository, FakeSecrets, stores};
    use crate::stores::token_key;

    fn service() -> AccountService<FakeRepository, FakeSecrets> {
        AccountService::new(stores())
    }

    fn service_with(secrets: FakeSecrets) -> AccountService<FakeRepository, FakeSecrets> {
        AccountService::new(Arc::new(AccountStores::new(
            FakeRepository::default(),
            secrets,
        )))
    }

    #[test]
    fn add_account_stores_and_returns_the_account() {
        let service = service();

        let added = service.add_account("ada@gmail.com", Some("Ada")).unwrap();

        assert_eq!(added.provider(), Provider::Gmail, "provider detected");
        assert_eq!(service.accounts().unwrap(), vec![added], "account listed");
    }

    #[test]
    fn add_account_rejects_duplicate_address_ignoring_domain_case() {
        let service = service();
        service.add_account("ada@example.com", None).unwrap();

        let result = service.add_account("ada@EXAMPLE.com", None);

        assert!(
            matches!(result, Err(EngineError::DuplicateAccount)),
            "duplicate must be rejected, got {result:?}"
        );
        assert_eq!(service.accounts().unwrap().len(), 1, "nothing added");
    }

    #[test]
    fn add_account_rejects_invalid_address() {
        let service = service();

        let result = service.add_account("not an address", None);

        assert!(
            matches!(
                result,
                Err(EngineError::InvalidInput(
                    DomainError::InvalidEmailAddress { .. }
                ))
            ),
            "invalid address must be rejected, got {result:?}"
        );
    }

    #[test]
    fn remove_account_deletes_existing_account() {
        let service = service();
        let added = service.add_account("ada@example.com", None).unwrap();

        service.remove_account(added.id()).unwrap();

        assert!(service.accounts().unwrap().is_empty(), "account removed");
    }

    #[test]
    fn remove_account_deletes_its_secrets_and_keeps_others() {
        let service = service();
        let removed = service.add_account("ada@example.com", None).unwrap();
        let kept = service.add_account("bob@example.com", None).unwrap();
        for account in [removed.id(), kept.id()] {
            service
                .stores
                .secrets
                .write(&token_key(account), &SecretString::from("token"))
                .unwrap();
        }

        service.remove_account(removed.id()).unwrap();

        let secrets = &service.stores.secrets;
        assert!(
            secrets.token(removed.id()).is_none(),
            "removed account's token deleted"
        );
        assert!(
            secrets.token(kept.id()).is_some(),
            "other account's token kept"
        );
    }

    #[test]
    fn remove_account_clears_the_cached_access_token() {
        let service = service();
        let added = service.add_account("ada@example.com", None).unwrap();
        service.stores.tokens.put(
            added.id(),
            AccessToken::new(SecretString::from("access"), UnixTime::from_seconds(9_999)),
        );

        service.remove_account(added.id()).unwrap();

        assert!(
            service.stores.tokens.get(added.id()).is_none(),
            "cache cleared"
        );
    }

    #[test]
    fn remove_account_keeps_the_account_when_secrets_cannot_be_deleted() {
        let service = service_with(FakeSecrets::failing());
        let added = service.add_account("ada@example.com", None).unwrap();

        let result = service.remove_account(added.id());

        assert!(
            matches!(result, Err(EngineError::Secrets(_))),
            "secret failure reported, got {result:?}"
        );
        assert_eq!(
            service.accounts().unwrap(),
            vec![added],
            "account still there"
        );
    }

    #[test]
    fn remove_account_of_an_unknown_id_is_not_found_even_when_secrets_fail() {
        let service = service_with(FakeSecrets::failing());

        let result = service.remove_account(AccountId::generate());

        assert!(
            matches!(result, Err(EngineError::AccountNotFound)),
            "existence is checked before the secret store, got {result:?}"
        );
    }

    #[test]
    fn remove_account_reports_missing_account() {
        let service = service();

        let result = service.remove_account(AccountId::generate());

        assert!(
            matches!(result, Err(EngineError::AccountNotFound)),
            "missing account must be reported, got {result:?}"
        );
    }

    #[test]
    fn storage_failures_are_wrapped() {
        let service = AccountService::new(Arc::new(AccountStores::new(
            FailingRepository,
            FakeSecrets::default(),
        )));

        let list = service.accounts();
        let add = service.add_account("ada@example.com", None);
        let remove = service.remove_account(AccountId::generate());

        assert!(
            matches!(list, Err(EngineError::Storage(_))),
            "list: {list:?}"
        );
        assert!(matches!(add, Err(EngineError::Storage(_))), "add: {add:?}");
        assert!(
            matches!(remove, Err(EngineError::Storage(_))),
            "remove: {remove:?}"
        );
    }

    #[test]
    fn error_messages_do_not_leak_addresses() {
        let service = service();
        service.add_account("secret@example.com", None).unwrap();

        let error = service.add_account("secret@example.com", None).unwrap_err();

        assert!(
            !error.to_string().contains("secret"),
            "message must not contain the address: {error}"
        );
    }
}
