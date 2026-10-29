use std::path::PathBuf;

use encore_core::StorageError;
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum SqliteError {
    #[error("failed to open database at {}", path.display())]
    Open {
        path: PathBuf,
        #[source]
        source: rusqlite::Error,
    },
    #[error("failed to restrict permissions on {}", path.display())]
    Permissions {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("database query failed")]
    Query(#[source] rusqlite::Error),
    #[error("database connection lock poisoned")]
    Poisoned,
    #[error("failed to run migrations")]
    Migrate(#[source] rusqlite_migration::Error),
}

impl From<SqliteError> for StorageError {
    fn from(error: SqliteError) -> Self {
        StorageError::Internal(Box::new(error))
    }
}
