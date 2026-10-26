use std::fmt;

use crate::id::uuid_id;
use crate::redact::redact_option;
use crate::{DomainError, EmailAddress, Provider};

uuid_id!(
    /// Identifies an account.
    AccountId,
    DomainError::InvalidAccountId
);

/// The data needed to add an account, after validation. `Debug` hides personal data.
#[derive(Clone, PartialEq, Eq)]
pub struct NewAccount {
    address: EmailAddress,
    display_name: Option<String>,
}

impl NewAccount {
    /// Validates user input for a new account.
    ///
    /// # Arguments
    ///
    /// * `address` - The email address as typed.
    /// * `display_name` - The sender name. Blank names are treated as absent.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::InvalidEmailAddress`] when the address is invalid.
    pub fn parse(address: &str, display_name: Option<&str>) -> Result<Self, DomainError> {
        let address = EmailAddress::parse(address)?;
        let display_name = display_name
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned);
        Ok(Self {
            address,
            display_name,
        })
    }

    /// Returns the validated address.
    #[must_use]
    pub const fn address(&self) -> &EmailAddress {
        &self.address
    }

    /// Turns the request into a stored account with a fresh id and a detected provider.
    #[must_use]
    pub fn into_account(self) -> Account {
        let provider = Provider::detect(&self.address);
        self.into_account_with(provider)
    }

    /// Turns the request into a stored account with a fresh id and a known provider.
    ///
    /// Signing in with Google makes a Gmail account even on a Google Workspace domain, which
    /// [`Provider::detect`] cannot recognise.
    ///
    /// # Arguments
    ///
    /// * `provider` - The provider that syncs the account.
    #[must_use]
    pub fn into_account_with(self, provider: Provider) -> Account {
        Account {
            id: AccountId::generate(),
            address: self.address,
            display_name: self.display_name,
            provider,
        }
    }
}

/// A mail account configured in the app. `Debug` hides personal data.
#[derive(Clone, PartialEq, Eq)]
pub struct Account {
    id: AccountId,
    address: EmailAddress,
    display_name: Option<String>,
    provider: Provider,
}

impl Account {
    /// Rebuilds an account from stored fields.
    ///
    /// # Arguments
    ///
    /// * `id` - The stored id.
    /// * `address` - The stored address.
    /// * `display_name` - The stored sender name.
    /// * `provider` - The stored provider.
    #[must_use]
    pub const fn from_parts(
        id: AccountId,
        address: EmailAddress,
        display_name: Option<String>,
        provider: Provider,
    ) -> Self {
        Self {
            id,
            address,
            display_name,
            provider,
        }
    }

    /// Returns the account id.
    #[must_use]
    pub const fn id(&self) -> AccountId {
        self.id
    }

    /// Returns the account address.
    #[must_use]
    pub const fn address(&self) -> &EmailAddress {
        &self.address
    }

    /// Returns the sender name, when one is set.
    #[must_use]
    pub fn display_name(&self) -> Option<&str> {
        self.display_name.as_deref()
    }

    /// Returns the provider that syncs this account.
    #[must_use]
    pub const fn provider(&self) -> Provider {
        self.provider
    }
}

impl fmt::Debug for NewAccount {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NewAccount")
            .field("address", &self.address)
            .field("display_name", &redact_option(self.display_name.as_ref()))
            .finish()
    }
}

impl fmt::Debug for Account {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Account")
            .field("id", &self.id)
            .field("address", &self.address)
            .field("display_name", &redact_option(self.display_name.as_ref()))
            .field("provider", &self.provider)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_ids_are_unique_and_ordered() {
        let first = AccountId::generate();
        let second = AccountId::generate();

        assert_ne!(first, second, "two generated ids must differ");
        assert!(first < second, "ids must sort in creation order");
    }

    #[test]
    fn account_id_round_trips_through_text() {
        let id = AccountId::generate();

        let parsed: AccountId = id.to_string().parse().unwrap();

        assert_eq!(parsed, id, "parsed id must equal the original");
    }

    #[test]
    fn account_id_rejects_invalid_text() {
        let result = "not-a-uuid".parse::<AccountId>();

        assert_eq!(
            result,
            Err(DomainError::InvalidAccountId),
            "invalid id text"
        );
    }

    #[test]
    fn new_account_drops_blank_display_name() {
        let request = NewAccount::parse("ada@example.com", Some("   ")).unwrap();

        let account = request.into_account();

        assert_eq!(account.display_name(), None, "blank name must be dropped");
    }

    #[test]
    fn new_account_trims_display_name_and_detects_provider() {
        let request = NewAccount::parse("ada@gmail.com", Some("  Ada  ")).unwrap();

        let account = request.into_account();

        assert_eq!(account.display_name(), Some("Ada"), "name must be trimmed");
        assert_eq!(account.provider(), Provider::Gmail, "provider from domain");
        assert_eq!(account.address().as_str(), "ada@gmail.com", "address kept");
    }

    #[test]
    fn into_account_with_keeps_the_given_provider() {
        let request = NewAccount::parse("ada@workspace.example", None).unwrap();

        let account = request.into_account_with(Provider::Gmail);

        assert_eq!(account.provider(), Provider::Gmail, "given, not detected");
    }

    #[test]
    fn new_account_rejects_invalid_address() {
        let result = NewAccount::parse("nope", None);

        assert!(result.is_err(), "invalid address must be rejected");
    }
}
