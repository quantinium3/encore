use encore_core::{Account, AccountId, AccountStore, StorageError};
use rusqlite::{OptionalExtension, Row, params};

use crate::{SqliteStore, error::SqliteError};

fn from_row(row: &Row<'_>) -> rusqlite::Result<Account> {
    Ok(Account {
        id: AccountId::new(row.get::<_, String>(0)?),
        session_cookie: row.get(1)?,
        added_at_unix: row.get(2)?,
    })
}


impl AccountStore for SqliteStore {
    fn save(&self, account: &Account) -> Result<(), StorageError> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO accounts (id, session_cookie, added_at_unix) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET session_cookie = excluded.session_cookie",
            params![
                account.id.as_str(),
                account.session_cookie,
                account.added_at_unix
            ],
        )
        .map_err(SqliteError::Query)?;
        Ok(())
    }
    fn list(&self) -> Result<Vec<Account>, StorageError> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, session_cookie, added_at_unix FROM accounts ORDER BY added_at_unix",
            )
            .map_err(SqliteError::Query)?;
        let accounts = stmt
            .query_map([], from_row)
            .map_err(SqliteError::Query)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(SqliteError::Query)?;
        Ok(accounts)
    }

    fn get(&self, id: &AccountId) -> Result<Account, StorageError> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT id, session_cookie, added_at_unix FROM accounts WHERE id = ?1",
            params![id.as_str()],
            from_row,
        )
        .optional()
        .map_err(SqliteError::Query)?
        .ok_or_else(|| StorageError::NotFound(id.clone()))
    }

    fn delete(&self, id: &AccountId) -> Result<(), StorageError> {
        let conn = self.conn()?;
        let deleted = conn
            .execute("DELETE FROM accounts WHERE id = ?1", params![id.as_str()])
            .map_err(SqliteError::Query)?;
        if deleted == 0 {
            return Err(StorageError::NotFound(id.clone()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(id: &str, cookie: &str, added_at_unix: i64) -> Account {
        Account {
            id: AccountId::new(id),
            session_cookie: cookie.into(),
            added_at_unix,
        }
    }

    #[test]
    fn saved_accounts_can_be_fetched() {
        let store = SqliteStore::open_in_memory().unwrap();
        let alice = account("alice", "cookie", 10);

        store.save(&alice).unwrap();

        assert_eq!(store.get(&alice.id).unwrap(), alice);
    }

    #[test]
    fn saving_again_updates_the_cookie_but_keeps_the_added_time() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.save(&account("alice", "old", 10)).unwrap();

        store.save(&account("alice", "new", 99)).unwrap();

        assert_eq!(
            store.get(&AccountId::new("alice")).unwrap(),
            account("alice", "new", 10)
        );
    }

    #[test]
    fn list_is_ordered_by_when_the_account_was_added() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.save(&account("b", "x", 20)).unwrap();
        store.save(&account("a", "x", 10)).unwrap();

        let ids: Vec<_> = store
            .list()
            .unwrap()
            .into_iter()
            .map(|account| account.id)
            .collect();

        assert_eq!(ids, [AccountId::new("a"), AccountId::new("b")]);
    }

    #[test]
    fn missing_accounts_are_not_found() {
        let store = SqliteStore::open_in_memory().unwrap();
        let id = AccountId::new("ghost");

        assert!(matches!(store.get(&id), Err(StorageError::NotFound(_))));
        assert!(matches!(store.delete(&id), Err(StorageError::NotFound(_))));
    }

    #[test]
    fn deleted_accounts_are_gone() {
        let store = SqliteStore::open_in_memory().unwrap();
        let alice = account("alice", "cookie", 10);
        store.save(&alice).unwrap();

        store.delete(&alice.id).unwrap();

        assert!(store.list().unwrap().is_empty());
    }
}
