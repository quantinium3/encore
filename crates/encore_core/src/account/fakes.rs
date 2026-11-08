use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use super::{Account, AccountId, AccountStore};
use crate::{
    Login, LoginError, LoginProvider, ProfileStore, RefreshError, Session, SessionKeeper,
    SessionRefresher, SessionValidator, StorageError, ValidationError, VerifiedSession,
};

pub struct FakeProvider(pub fn() -> Result<Login, LoginError>);

impl LoginProvider for FakeProvider {
    fn login(&self) -> Result<Login, LoginError> {
        (self.0)()
    }
}

pub struct FakeValidator(pub fn() -> Result<AccountId, ValidationError>);

impl SessionValidator for FakeValidator {
    fn validate(&self, _: &Session) -> Result<VerifiedSession, ValidationError> {
        (self.0)().map(VerifiedSession::new)
    }
}

pub struct VerifiedValidator(pub fn() -> Result<VerifiedSession, ValidationError>);

impl SessionValidator for VerifiedValidator {
    fn validate(&self, _: &Session) -> Result<VerifiedSession, ValidationError> {
        (self.0)()
    }
}

pub struct CookieValidator(pub fn(&str) -> Result<AccountId, ValidationError>);

impl SessionValidator for CookieValidator {
    fn validate(&self, session: &Session) -> Result<VerifiedSession, ValidationError> {
        (self.0)(session.cookie()).map(VerifiedSession::new)
    }
}

pub struct CookieVerifiedValidator(pub fn(&str) -> Result<VerifiedSession, ValidationError>);

impl SessionValidator for CookieVerifiedValidator {
    fn validate(&self, session: &Session) -> Result<VerifiedSession, ValidationError> {
        (self.0)(session.cookie())
    }
}

pub struct FakeRefresher(pub fn(&AccountId, &Session) -> Result<Session, RefreshError>);

impl SessionRefresher for FakeRefresher {
    fn refresh(&self, account: &AccountId, current: &Session) -> Result<Session, RefreshError> {
        (self.0)(account, current)
    }
}

#[derive(Clone, Default)]
pub struct FakeProfiles {
    events: Rc<RefCell<Vec<String>>>,
    fail_adopt: bool,
}

impl FakeProfiles {
    pub fn staged() -> PathBuf {
        PathBuf::from("staged")
    }

    pub fn failing_adopt() -> Self {
        Self {
            fail_adopt: true,
            ..Self::default()
        }
    }

    pub fn events(&self) -> Vec<String> {
        self.events.borrow().clone()
    }

    fn record(&self, event: String) {
        self.events.borrow_mut().push(event);
    }
}

impl ProfileStore for FakeProfiles {
    fn adopt(&self, staged: &Path, id: &AccountId) -> io::Result<()> {
        if self.fail_adopt {
            return Err(io::Error::other("disk full"));
        }
        self.record(format!("adopt {} {id}", staged.display()));
        Ok(())
    }

    fn discard(&self, staged: &Path) {
        self.record(format!("discard {}", staged.display()));
    }

    fn remove(&self, id: &AccountId) -> io::Result<()> {
        self.record(format!("remove {id}"));
        Ok(())
    }
}

#[derive(Default)]
pub struct FakeStore {
    accounts: RefCell<BTreeMap<String, Account>>,
    active: RefCell<Option<AccountId>>,
    fail_save: bool,
}

impl FakeStore {
    pub fn failing_save() -> Self {
        Self {
            fail_save: true,
            ..Self::default()
        }
    }

    pub fn set_active_unchecked(&self, id: Option<AccountId>) {
        *self.active.borrow_mut() = id;
    }
}

impl AccountStore for FakeStore {
    fn save(&self, account: &Account) -> Result<(), StorageError> {
        if self.fail_save {
            return Err(StorageError::Internal("disk full".into()));
        }
        let mut accounts = self.accounts.borrow_mut();
        let added_at_unix = accounts
            .get(account.id.as_str())
            .map_or(account.added_at_unix, |existing| existing.added_at_unix);
        accounts.insert(
            account.id.to_string(),
            Account {
                added_at_unix,
                ..account.clone()
            },
        );
        Ok(())
    }

    fn list(&self) -> Result<Vec<Account>, StorageError> {
        Ok(self.accounts.borrow().values().cloned().collect())
    }

    fn get(&self, id: &AccountId) -> Result<Option<Account>, StorageError> {
        Ok(self.accounts.borrow().get(id.as_str()).cloned())
    }

    fn delete(&self, id: &AccountId) -> Result<(), StorageError> {
        if self.accounts.borrow_mut().remove(id.as_str()).is_none() {
            return Err(StorageError::NotFound(id.clone()));
        }
        if self.active.borrow().as_ref() == Some(id) {
            *self.active.borrow_mut() = None;
        }
        Ok(())
    }

    fn active(&self) -> Result<Option<AccountId>, StorageError> {
        Ok(self.active.borrow().clone())
    }

    fn set_active(&self, id: Option<&AccountId>) -> Result<(), StorageError> {
        if let Some(id) = id
            && !self.accounts.borrow().contains_key(id.as_str())
        {
            return Err(StorageError::NotFound(id.clone()));
        }
        *self.active.borrow_mut() = id.cloned();
        Ok(())
    }
}

#[derive(Clone, Default)]
pub struct FakeKeeper(pub Rc<RefCell<Vec<String>>>);

impl FakeKeeper {
    pub fn events(&self) -> Vec<String> {
        self.0.borrow().clone()
    }
}

impl SessionKeeper for FakeKeeper {
    fn start(&self, account: &AccountId, session: &Session) {
        self.0
            .borrow_mut()
            .push(format!("start {account} {}", session.cookie()));
    }

    fn stop(&self) {
        self.0.borrow_mut().push("stop".to_owned());
    }
}
