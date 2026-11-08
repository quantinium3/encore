#[cfg(test)]
pub(crate) mod fakes;
mod login;
mod model;
mod restore;
mod rotation;
mod service;
mod store;

pub use login::{SignInError, sign_in};
pub use model::{Account, AccountId, AccountProfile};
pub use restore::{SessionStatus, restore_session};
pub use rotation::{EventOutcome, apply_event};
pub use service::{AccountService, Accounts, sign_out};
pub use store::AccountStore;
