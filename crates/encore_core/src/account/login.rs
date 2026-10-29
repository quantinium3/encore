use std::time::{SystemTime, UNIX_EPOCH};

use thiserror::Error;

use crate::StorageError;

#[derive(Debug, Error)]
pub enum SignInError {
    #[error(transparent)]
    Login(#[from] LoginError)),
    #[error("the session has no account identity; sign in again")]
    NoIdentity,
    #[error("YouTube rejected the session; sign in again")]
    Rejected,
    #[error("could not check the session with YouTube: {0}")]
    Validation(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error(transparent)]
    Storage(#[from] StorageError),
}
