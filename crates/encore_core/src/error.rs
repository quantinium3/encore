use thiserror::Error;

use super::AccountId;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("account not found: {0}")]
    NotFound(AccountId),
    #[error("internal storage error")]
    Internal(#[source] Box<dyn std::error::Error + Send + Sync>),
}
