use thiserror::Error;

use super::Session;
use crate::AccountId;

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("the session was rejected")]
    Rejected,
    #[error("could not validate the session: {0}")]
    Backend(#[source] Box<dyn std::error::Error + Send + Sync>),
}

pub trait SessionValidator {
    fn validate(&self, session: &Session) -> Result<AccountId, ValidationError>;
}
