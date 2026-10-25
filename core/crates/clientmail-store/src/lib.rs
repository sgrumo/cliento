//! `SQLite` persistence for the `ClientMail` core.
//!
//! [`SqliteStore`] implements the repository traits of `clientmail-domain`. It holds no business
//! rules.

mod accounts;
mod error;
mod migrations;
mod store;

pub use error::StoreError;
pub use store::SqliteStore;
