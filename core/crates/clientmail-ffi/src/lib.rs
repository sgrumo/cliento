//! `UniFFI` API of the `ClientMail` core, consumed by the Swift app.
//!
//! This crate composes the store and the engine and converts their types to FFI-friendly ones.
//! It holds no business rules.

// UniFFI's generated scaffolding uses `unsafe` to cross the C boundary. Handwritten code in this
// crate must stay safe.
#![allow(
    unsafe_code,
    reason = "UniFFI scaffolding is generated unsafe FFI glue"
)]

mod client;
mod error;
mod records;
mod runtime;
mod secrets;
mod sign_in;

pub use client::CoreClient;
pub use error::CoreError;
pub use records::{AccountRecord, ProviderKind};
pub use secrets::{SecretBridgeError, SecretStoreBridge};
pub use sign_in::GoogleSignInSession;

uniffi::setup_scaffolding!();

/// Returns the version of the Rust core, for the About window and bug reports.
#[uniffi::export]
#[must_use]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}
