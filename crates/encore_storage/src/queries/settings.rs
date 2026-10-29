use encore_core::{SettingKey, SettingsStore, StorageError};
use rusqlite::{OptionalExtension, params};

use crate::{SqliteStore, error::SqliteError};

impl SettingsStore for SqliteStore {
    fn setting(&self, key: SettingKey) -> Result<Option<String>, StorageError> {
        let conn = self.conn()?;
        let value = conn
            .query_row(
                "SELECT value FROM settings WHERE category = ?1 AND key = ?2",
                params![key.category, key.name],
                |row| row.get(0),
            )
            .optional()
            .map_err(SqliteError::Query)?;
        Ok(value)
    }

    fn set_setting(&self, key: SettingKey, value: &str) -> Result<(), StorageError> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO settings (category, key, value) VALUES (?1, ?2, ?3)
             ON CONFLICT(category, key) DO UPDATE SET value = excluded.value",
            params![key.category, key.name, value],
        )
        .map_err(SqliteError::Query)?;
        Ok(())
    }

    fn remove_setting(&self, key: SettingKey) -> Result<(), StorageError> {
        let conn = self.conn()?;
        conn.execute(
            "DELETE FROM settings WHERE category = ?1 AND key = ?2",
            params![key.category, key.name],
        )
        .map_err(SqliteError::Query)?;
        Ok(())
    }

    fn settings_in(&self, category: &str) -> Result<Vec<(String, String)>, StorageError> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare("SELECT key, value FROM settings WHERE category = ?1 ORDER BY key")
            .map_err(SqliteError::Query)?;
        let settings = stmt
            .query_map(params![category], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(SqliteError::Query)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(SqliteError::Query)?;
        Ok(settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const THEME: SettingKey = SettingKey::new("appearance", "theme");
    const SIDEBAR: SettingKey = SettingKey::new("appearance", "sidebar");
    const PLAYER_THEME: SettingKey = SettingKey::new("player", "theme");

    #[test]
    fn unset_settings_are_absent() {
        let store = SqliteStore::open_in_memory().unwrap();

        assert_eq!(store.setting(THEME).unwrap(), None);
    }

    #[test]
    fn set_settings_can_be_read_back() {
        let store = SqliteStore::open_in_memory().unwrap();

        store.set_setting(THEME, "dark").unwrap();

        assert_eq!(store.setting(THEME).unwrap().as_deref(), Some("dark"));
    }

    #[test]
    fn setting_again_overwrites_the_value() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.set_setting(THEME, "dark").unwrap();

        store.set_setting(THEME, "light").unwrap();

        assert_eq!(store.setting(THEME).unwrap().as_deref(), Some("light"));
    }

    #[test]
    fn the_same_name_in_different_categories_is_independent() {
        let store = SqliteStore::open_in_memory().unwrap();

        store.set_setting(THEME, "dark").unwrap();
        store.set_setting(PLAYER_THEME, "light").unwrap();

        assert_eq!(store.setting(THEME).unwrap().as_deref(), Some("dark"));
        assert_eq!(store.setting(PLAYER_THEME).unwrap().as_deref(), Some("light"));
    }

    #[test]
    fn removing_a_setting_leaves_the_rest_and_is_idempotent() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.set_setting(THEME, "dark").unwrap();
        store.set_setting(SIDEBAR, "open").unwrap();

        store.remove_setting(THEME).unwrap();
        store.remove_setting(THEME).unwrap();

        assert_eq!(store.setting(THEME).unwrap(), None);
        assert_eq!(store.setting(SIDEBAR).unwrap().as_deref(), Some("open"));
    }

    #[test]
    fn a_category_lists_only_its_own_settings_ordered_by_name() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.set_setting(THEME, "dark").unwrap();
        store.set_setting(SIDEBAR, "open").unwrap();
        store.set_setting(PLAYER_THEME, "light").unwrap();

        assert_eq!(
            store.settings_in("appearance").unwrap(),
            [
                ("sidebar".to_owned(), "open".to_owned()),
                ("theme".to_owned(), "dark".to_owned()),
            ]
        );
        assert!(store.settings_in("missing").unwrap().is_empty());
    }
}
