use clientmail_domain::ports::AccountRepository;
use clientmail_domain::{Account, AccountId, NewAccount};

use crate::EngineError;

/// Adds, lists and removes accounts.
///
/// Generic over the repository so tests can pass a fake. `R` is resolved at compile time, so this
/// costs nothing at runtime compared to a concrete type.
#[derive(Debug)]
pub struct AccountService<R> {
    repository: R,
}

impl<R: AccountRepository> AccountService<R> {
    /// Creates the service.
    ///
    /// # Arguments
    ///
    /// * `repository` - Where accounts are stored.
    #[must_use]
    pub const fn new(repository: R) -> Self {
        Self { repository }
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

    /// Removes an account.
    ///
    /// # Arguments
    ///
    /// * `id` - The account to remove.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError::AccountNotFound`] when no account has this id, and
    /// [`EngineError::Storage`] when the repository fails.
    pub fn remove_account(&self, id: AccountId) -> Result<(), EngineError> {
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
    use std::fmt;

    use clientmail_domain::{DomainError, EmailAddress, Provider};

    use super::*;

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

        fn delete(&self, _id: AccountId) -> Result<bool, FakeError> {
            Err(FakeError)
        }
    }

    #[test]
    fn add_account_stores_and_returns_the_account() {
        let service = AccountService::new(FakeRepository::default());

        let added = service.add_account("ada@gmail.com", Some("Ada")).unwrap();

        assert_eq!(added.provider(), Provider::Gmail, "provider detected");
        assert_eq!(service.accounts().unwrap(), vec![added], "account listed");
    }

    #[test]
    fn add_account_rejects_duplicate_address_ignoring_domain_case() {
        let service = AccountService::new(FakeRepository::default());
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
        let service = AccountService::new(FakeRepository::default());

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
        let service = AccountService::new(FakeRepository::default());
        let added = service.add_account("ada@example.com", None).unwrap();

        service.remove_account(added.id()).unwrap();

        assert!(service.accounts().unwrap().is_empty(), "account removed");
    }

    #[test]
    fn remove_account_reports_missing_account() {
        let service = AccountService::new(FakeRepository::default());

        let result = service.remove_account(AccountId::generate());

        assert!(
            matches!(result, Err(EngineError::AccountNotFound)),
            "missing account must be reported, got {result:?}"
        );
    }

    #[test]
    fn storage_failures_are_wrapped() {
        let service = AccountService::new(FailingRepository);

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
        let service = AccountService::new(FakeRepository::default());
        service.add_account("secret@example.com", None).unwrap();

        let error = service.add_account("secret@example.com", None).unwrap_err();

        assert!(
            !error.to_string().contains("secret"),
            "message must not contain the address: {error}"
        );
    }
}
