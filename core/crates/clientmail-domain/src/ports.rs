//! Traits the engine uses to reach storage and the network.
//!
//! Infrastructure crates implement them; the engine and the tests depend only on the traits.

use std::error::Error;

use crate::{Account, AccountId, EmailAddress};

/// Persists accounts.
///
/// Implementations decide the storage format. They hold no business rules: duplicate checks and
/// validation happen in the engine.
pub trait AccountRepository {
    /// The storage error type. `Send + Sync + 'static` lets the engine wrap it in its own error.
    type Error: Error + Send + Sync + 'static;

    /// Returns every account, oldest first.
    ///
    /// # Errors
    ///
    /// Returns the storage error when the read fails.
    fn list(&self) -> Result<Vec<Account>, Self::Error>;

    /// Returns the account with this address, if any.
    ///
    /// # Arguments
    ///
    /// * `address` - The normalised address to look up.
    ///
    /// # Errors
    ///
    /// Returns the storage error when the read fails.
    fn find_by_address(&self, address: &EmailAddress) -> Result<Option<Account>, Self::Error>;

    /// Stores a new account.
    ///
    /// # Arguments
    ///
    /// * `account` - The account to store.
    ///
    /// # Errors
    ///
    /// Returns the storage error when the write fails.
    fn insert(&self, account: &Account) -> Result<(), Self::Error>;

    /// Deletes an account.
    ///
    /// # Arguments
    ///
    /// * `id` - The account to delete.
    ///
    /// # Returns
    ///
    /// `true` when an account was deleted, `false` when none had this id.
    ///
    /// # Errors
    ///
    /// Returns the storage error when the write fails.
    fn delete(&self, id: AccountId) -> Result<bool, Self::Error>;
}
