//! Use cases of the `ClientMail` core.
//!
//! The engine applies business rules and reaches storage and the network only through the port
//! traits of `clientmail-domain`, so every use case is testable with fakes.

mod accounts;
mod error;

pub use accounts::AccountService;
pub use error::EngineError;
