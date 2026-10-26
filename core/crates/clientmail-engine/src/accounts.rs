use clientmail_domain::ports::{AccountRepository, SecretStore};
use clientmail_domain::{Account, AccountId, NewAccount};

use crate::EngineError;

/// Adds, lists and removes accounts.
///
/// Generic over the repository and the secret store so tests can pass fakes. `R` and `S` are
/// resolved at compile time, so this costs nothing at runtime compared to concrete types.
#[derive(Debug)]
pub struct AccountService<R, S> {
    repository: R,
    secrets: S,
}

impl<R: AccountRepository, S: SecretStore> AccountService<R, S> {
    /// Creates the service.
    ///
    /// # Arguments
    ///
    /// * `repository` - Where accounts are stored.
    /// * `secrets` - Where account secrets are stored.
    #[must_use]
    pub const fn new(repository: R, secrets: S) -> Self {
        Self {
            repository,
            secrets,
        }
    }

    /// Returns every account, oldest first.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError::Storage`] when the repository fails.
    pub fn accounts(&self) -> Result<Vec<Account>, EngineError> {
        self.repository.list().map_err(EngineError::storage)
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
        let existing = self
            .repository
            .find_by_address(request.address())
            .map_err(EngineError::storage)?;
        if existing.is_some() {
            return Err(EngineError::DuplicateAccount);
        }
        let account = request.into_account();
        self.repository
            .insert(&account)
            .map_err(EngineError::storage)?;
        Ok(account)
    }

    /// Removes an account and its secrets.
    ///
    /// The account must exist; then its secrets go first, so that if they cannot be deleted the
    /// account stays and the user can retry, instead of a token outliving the account.
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
        if !self.repository.exists(id).map_err(EngineError::storage)? {
            return Err(EngineError::AccountNotFound);
        }
        self.secrets
            .delete_account(id)
            .map_err(EngineError::secrets)?;
        let deleted = self.repository.delete(id).map_err(EngineError::storage)?;
        if deleted {
            Ok(())
        } else {
            Err(EngineError::AccountNotFound)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::fmt;

    use clientmail_domain::{
        DomainError, EmailAddress, ExposeSecret, Provider, SecretKey, SecretKind, SecretString,
    };

    use super::*;

    /// In-memory secret store keyed by label. With `failing` set, every call fails.
    #[derive(Default)]
    struct FakeSecrets {
        items: RefCell<HashMap<String, String>>,
        failing: bool,
    }

    impl FakeSecrets {
        fn failing() -> Self {
            Self {
                failing: true,
                ..Self::default()
            }
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
                .borrow()
                .get(&key.label())
                .map(|value| SecretString::from(value.clone())))
        }

        fn write(&self, key: &SecretKey, secret: &SecretString) -> Result<(), FakeError> {
            self.check()?;
            self.items
                .borrow_mut()
                .insert(key.label(), secret.expose_secret().to_owned());
            Ok(())
        }

        fn delete(&self, key: &SecretKey) -> Result<bool, FakeError> {
            self.check()?;
            Ok(self.items.borrow_mut().remove(&key.label()).is_some())
        }
    }

    fn service() -> AccountService<FakeRepository, FakeSecrets> {
        AccountService::new(FakeRepository::default(), FakeSecrets::default())
    }

    fn token_key(account: AccountId) -> SecretKey {
        SecretKey::new(account, SecretKind::OAuthRefreshToken)
    }

    /// In-memory repository. `RefCell` allows mutation behind `&self`, checked at runtime.
    #[derive(Default)]
    struct FakeRepository {
        accounts: RefCell<Vec<Account>>,
    }

    #[derive(Debug)]
    struct FakeError;

    impl fmt::Display for FakeError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("fake failure")
        }
    }

    impl std::error::Error for FakeError {}

    impl AccountRepository for FakeRepository {
        type Error = FakeError;

        fn list(&self) -> Result<Vec<Account>, FakeError> {
            Ok(self.accounts.borrow().clone())
        }

        fn find_by_address(&self, address: &EmailAddress) -> Result<Option<Account>, FakeError> {
            Ok(self
                .accounts
                .borrow()
                .iter()
                .find(|account| account.address() == address)
                .cloned())
        }

        fn insert(&self, account: &Account) -> Result<(), FakeError> {
            self.accounts.borrow_mut().push(account.clone());
            Ok(())
        }

        fn exists(&self, id: AccountId) -> Result<bool, FakeError> {
            Ok(self
                .accounts
                .borrow()
                .iter()
                .any(|account| account.id() == id))
        }

        fn delete(&self, id: AccountId) -> Result<bool, FakeError> {
            let mut accounts = self.accounts.borrow_mut();
            let before = accounts.len();
            accounts.retain(|account| account.id() != id);
            Ok(accounts.len() < before)
        }
    }

    /// Repository whose every call fails.
    struct FailingRepository;

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
                .secrets
                .write(&token_key(account), &SecretString::from("token"))
                .unwrap();
        }

        service.remove_account(removed.id()).unwrap();

        assert!(
            service
                .secrets
                .read(&token_key(removed.id()))
                .unwrap()
                .is_none(),
            "removed account's token deleted"
        );
        assert!(
            service
                .secrets
                .read(&token_key(kept.id()))
                .unwrap()
                .is_some(),
            "other account's token kept"
        );
    }

    #[test]
    fn remove_account_keeps_the_account_when_secrets_cannot_be_deleted() {
        let service = AccountService::new(FakeRepository::default(), FakeSecrets::failing());
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
        let service = AccountService::new(FakeRepository::default(), FakeSecrets::failing());

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
        let service = AccountService::new(FailingRepository, FakeSecrets::default());

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
