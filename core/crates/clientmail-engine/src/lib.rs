//! Use cases of the `ClientMail` core.
//!
//! The engine applies business rules and reaches storage and the network only through the port
//! traits of `clientmail-domain`, so every use case is testable with fakes.

mod accounts;
mod error;
#[cfg(test)]
mod fakes;
mod sign_in;
mod stores;
mod tokens;

pub use accounts::AccountService;
pub use error::{EngineError, SignInProblem};
pub use sign_in::{GoogleSignIn, PendingSignIn};
pub use stores::AccountStores;
pub use tokens::TokenService;
