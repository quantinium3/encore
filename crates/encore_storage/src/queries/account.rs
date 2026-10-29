use encore_core::{Account, AccountId, AccountStore, SettingKey, SettingsStore, StorageError};
use rusqlite::{OptionalExtension, Row, params};

use crate::{SqliteStore, error::SqliteError};

const ACTIVE_ACCOUNT: SettingKey = SettingKey::new("account", "active");

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

    fn get(&self, id: &AccountId) -> Result<Option<Account>, StorageError> {
        let conn = self.conn()?;
        let account = conn
            .query_row(
                "SELECT id, session_cookie, added_at_unix FROM accounts WHERE id = ?1",
                params![id.as_str()],
                from_row,
            )
            .optional()
            .map_err(SqliteError::Query)?;
        Ok(account)
    }

    fn delete(&self, id: &AccountId) -> Result<(), StorageError> {
        let conn = self.conn()?;
        let tx = conn.unchecked_transaction().map_err(SqliteError::Query)?;
        let deleted = tx
            .execute("DELETE FROM accounts WHERE id = ?1", params![id.as_str()])
            .map_err(SqliteError::Query)?;
        if deleted == 0 {
            return Err(StorageError::NotFound(id.clone()));
        }
        tx.execute(
            "DELETE FROM settings WHERE category = ?1 AND key = ?2 AND value = ?3",
            params![ACTIVE_ACCOUNT.category, ACTIVE_ACCOUNT.name, id.as_str()],
        )
        .map_err(SqliteError::Query)?;
        tx.commit().map_err(SqliteError::Query)?;
        Ok(())
    }

    fn active(&self) -> Result<Option<AccountId>, StorageError> {
        let conn = self.conn()?;
        let id = conn
            .query_row(
                "SELECT accounts.id FROM settings
                 JOIN accounts ON accounts.id = settings.value
                 WHERE settings.category = ?1 AND settings.key = ?2",
                params![ACTIVE_ACCOUNT.category, ACTIVE_ACCOUNT.name],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(SqliteError::Query)?;
        Ok(id.map(AccountId::new))
    }

    fn set_active(&self, id: Option<&AccountId>) -> Result<(), StorageError> {
        let Some(id) = id else {
            return self.remove_setting(ACTIVE_ACCOUNT);
        };
        let conn = self.conn()?;
        let changed = conn
            .execute(
                "INSERT INTO settings (category, key, value)
                 SELECT ?1, ?2, id FROM accounts WHERE id = ?3
                 ON CONFLICT(category, key) DO UPDATE SET value = excluded.value",
                params![ACTIVE_ACCOUNT.category, ACTIVE_ACCOUNT.name, id.as_str()],
            )
            .map_err(SqliteError::Query)?;
        if changed == 0 {
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

        assert_eq!(store.get(&alice.id).unwrap(), Some(alice));
    }

    #[test]
    fn saving_again_updates_the_cookie_but_keeps_the_added_time() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.save(&account("alice", "old", 10)).unwrap();

        store.save(&account("alice", "new", 99)).unwrap();

        assert_eq!(
            store.get(&AccountId::new("alice")).unwrap(),
            Some(account("alice", "new", 10))
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
    fn missing_accounts_are_absent_and_cannot_be_deleted() {
        let store = SqliteStore::open_in_memory().unwrap();
        let id = AccountId::new("ghost");

        assert_eq!(store.get(&id).unwrap(), None);
        assert!(matches!(store.delete(&id), Err(StorageError::NotFound(_))));
    }

    #[test]
    fn deleting_an_account_leaves_the_others() {
        let store = SqliteStore::open_in_memory().unwrap();
        let alice = account("alice", "cookie", 10);
        let bob = account("bob", "cookie", 20);
        store.save(&alice).unwrap();
        store.save(&bob).unwrap();

        store.delete(&alice.id).unwrap();

        assert_eq!(store.get(&alice.id).unwrap(), None);
        assert_eq!(store.list().unwrap(), [bob]);
    }

    #[test]
    fn there_is_no_active_account_until_one_is_chosen() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.save(&account("alice", "cookie", 10)).unwrap();

        assert_eq!(store.active().unwrap(), None);
    }

    #[test]
    fn the_active_account_can_be_switched_and_cleared() {
        let store = SqliteStore::open_in_memory().unwrap();
        let alice = AccountId::new("alice");
        let bob = AccountId::new("bob");
        store.save(&account("alice", "cookie", 10)).unwrap();
        store.save(&account("bob", "cookie", 20)).unwrap();

        store.set_active(Some(&alice)).unwrap();
        assert_eq!(store.active().unwrap(), Some(alice));

        store.set_active(Some(&bob)).unwrap();
        assert_eq!(store.active().unwrap(), Some(bob));

        store.set_active(None).unwrap();
        assert_eq!(store.active().unwrap(), None);
    }

    #[test]
    fn an_unknown_account_cannot_become_active() {
        let store = SqliteStore::open_in_memory().unwrap();
        let ghost = AccountId::new("ghost");

        assert!(matches!(
            store.set_active(Some(&ghost)),
            Err(StorageError::NotFound(_))
        ));
        assert_eq!(store.active().unwrap(), None);
    }

    #[test]
    fn deleting_the_active_account_clears_it_for_good() {
        let store = SqliteStore::open_in_memory().unwrap();
        let alice = account("alice", "cookie", 10);
        store.save(&alice).unwrap();
        store.set_active(Some(&alice.id)).unwrap();

        store.delete(&alice.id).unwrap();
        store.save(&alice).unwrap();

        assert_eq!(store.active().unwrap(), None);
    }

    #[test]
    fn deleting_another_account_keeps_the_active_one() {
        let store = SqliteStore::open_in_memory().unwrap();
        let alice = account("alice", "cookie", 10);
        let bob = account("bob", "cookie", 20);
        store.save(&alice).unwrap();
        store.save(&bob).unwrap();
        store.set_active(Some(&alice.id)).unwrap();

        store.delete(&bob.id).unwrap();

        assert_eq!(store.active().unwrap(), Some(alice.id));
    }
}
