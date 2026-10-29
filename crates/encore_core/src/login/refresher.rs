use thiserror::Error;

use super::Session;
use crate::AccountId;

#[derive(Debug, Error)]
pub enum RefreshError {
    #[error("the login session is no longer signed in")]
    NotSignedIn,
    #[error("there is no newer session to offer")]
    Unchanged,
    #[error("could not refresh the session: {0}")]
    Backend(#[source] Box<dyn std::error::Error + Send + Sync>),
}

pub trait SessionRefresher {
    fn refresh(&self, account: &AccountId, current: &Session) -> Result<Session, RefreshError>;
}
