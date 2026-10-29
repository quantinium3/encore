pub trait AccountStore {
    fn save(&self, account: &Account) -> Result<(), StorageError>;
    fn list(&self) -> Result<Vec<Account>, StorageError>;
    fn get(&self, id: &AccountId) -> Result<Option<Account>, StorageError>;
    fn remove(&self, id: &AccountId) -> Result<(), StorageError>;
    fn active(&self) -> Result<Option<AccountId>, StorageError>;
    fn set_active(&self, id: Option<&AccountId>) -> Result<(), StorageError>;
}
