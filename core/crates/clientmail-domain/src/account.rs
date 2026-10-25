use std::fmt;
use std::str::FromStr;

use uuid::Uuid;

use crate::{DomainError, EmailAddress, Provider};

/// Identifies an account.
///
/// Backed by a UUID v7, so ids sort in creation order without a separate timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AccountId(Uuid);

impl AccountId {
    /// Creates a new, unique id.
    #[must_use]
    pub fn generate() -> Self {
        Self(Uuid::now_v7())
    }
}

impl fmt::Display for AccountId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for AccountId {
    type Err = DomainError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(input)
            .map(Self)
            .map_err(|_| DomainError::InvalidAccountId)
    }
}

/// The data needed to add an account, after validation.
#[derive(Debug, Clone, PartialEq, Eq)]
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
        Account {
            id: AccountId::generate(),
            address: self.address,
            display_name: self.display_name,
            provider,
        }
    }
}

/// A mail account configured in the app.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    fn new_account_rejects_invalid_address() {
        let result = NewAccount::parse("nope", None);

        assert!(result.is_err(), "invalid address must be rejected");
    }
}
