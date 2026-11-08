use std::sync::Arc;

use crate::{Account, AccountId, StorageError};

pub trait AccountStore {
    fn save(&self, account: &Account) -> Result<(), StorageError>;
    fn list(&self) -> Result<Vec<Account>, StorageError>;
    fn get(&self, id: &AccountId) -> Result<Option<Account>, StorageError>;
    fn delete(&self, id: &AccountId) -> Result<(), StorageError>;
    fn active(&self) -> Result<Option<AccountId>, StorageError>;
    fn set_active(&self, id: Option<&AccountId>) -> Result<(), StorageError>;
}

impl<S: AccountStore + ?Sized> AccountStore for Arc<S> {
    fn save(&self, account: &Account) -> Result<(), StorageError> {
        (**self).save(account)
    }

    fn list(&self) -> Result<Vec<Account>, StorageError> {
        (**self).list()
    }

    fn get(&self, id: &AccountId) -> Result<Option<Account>, StorageError> {
        (**self).get(id)
    }

    fn delete(&self, id: &AccountId) -> Result<(), StorageError> {
        (**self).delete(id)
    }

    fn active(&self) -> Result<Option<AccountId>, StorageError> {
        (**self).active()
    }

    fn set_active(&self, id: Option<&AccountId>) -> Result<(), StorageError> {
        (**self).set_active(id)
    }
}
