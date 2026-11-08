use thiserror::Error;

use super::Session;
use crate::{AccountId, AccountProfile};

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("the session was rejected")]
    Rejected,
    #[error("could not validate the session: {0}")]
    Backend(#[source] Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedSession {
    pub id: AccountId,
    pub profile: AccountProfile,
}

impl VerifiedSession {
    pub fn new(id: AccountId) -> Self {
        Self {
            id,
            profile: AccountProfile::default(),
        }
    }
}

pub trait SessionValidator {
    fn validate(&self, session: &Session) -> Result<VerifiedSession, ValidationError>;
}
