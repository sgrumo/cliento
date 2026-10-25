//! Domain types, validation and port traits of the `ClientMail` core.
//!
//! This crate performs no I/O. Storage and network crates implement the traits in [`ports`].

mod account;
mod address;
mod error;
pub mod ports;
mod provider;

pub use account::{Account, AccountId, NewAccount};
pub use address::EmailAddress;
pub use error::DomainError;
pub use provider::Provider;
