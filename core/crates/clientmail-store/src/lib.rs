//! `SQLite` persistence for the `ClientMail` core.
//!
//! [`SqliteStore`] implements the repository traits of `clientmail-domain`, and [`FileBlobStore`]
//! keeps message bodies as content-addressed files. Neither holds business rules.

mod accounts;
mod blobs;
mod error;
mod mailboxes;
mod mapping;
mod message_rows;
mod messages;
mod migrations;
mod store;

pub use blobs::FileBlobStore;
pub use error::StoreError;
pub use store::SqliteStore;
