use std::{
    path::Path,
    sync::{Mutex, MutexGuard},
};

use encore_core::StorageError;
use rusqlite::Connection;

use crate::{error::SqliteError, migrations};

pub struct SqliteStore {
    conn: Mutex<Connection>,
}

impl SqliteStore {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let path = path.as_ref();
        let conn = Connection::open(path).map_err(|source| SqliteError::Open {
            path: path.into(),
            source,
        })?;
        restrict_permissions(path).map_err(|source| SqliteError::Permissions {
            path: path.into(),
            source,
        })?;
        Self::from_connection(conn)
    }

    fn from_connection(mut conn: Connection) -> Result<Self, StorageError> {
        migrations::run(&mut conn).map_err(SqliteError::Migrate)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub(crate) fn conn(&self) -> Result<MutexGuard<'_, Connection>, SqliteError> {
        self.conn.lock().map_err(|_| SqliteError::Poisoned)
    }

    pub fn open_in_memory() -> Result<Self, StorageError> {
        Self::from_connection(
            Connection::open_in_memory().map_err(|source| SqliteError::Open {
                path: ":memory:".into(),
                source,
            })?,
        )
    }
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn restrict_permissions(_: &Path) -> std::io::Result<()> {
    Ok(())
}
