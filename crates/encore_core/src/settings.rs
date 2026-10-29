use crate::StorageError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SettingKey {
    pub category: &'static str,
    pub name: &'static str,
}

impl SettingKey {
    pub const fn new(category: &'static str, name: &'static str) -> Self {
        Self { category, name }
    }
}

pub trait SettingsStore {
    fn setting(&self, key: SettingKey) -> Result<Option<String>, StorageError>;
    fn set_setting(&self, key: SettingKey, value: &str) -> Result<(), StorageError>;
    fn remove_setting(&self, key: SettingKey) -> Result<(), StorageError>;
    fn settings_in(&self, category: &str) -> Result<Vec<(String, String)>, StorageError>;
}
