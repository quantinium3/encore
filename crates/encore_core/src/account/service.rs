use super::{
    Account, AccountId, AccountStore, EventOutcome, SessionStatus, SignInError, apply_event,
    restore_session, sign_in,
};
use crate::{
    LoginProvider, ProfileStore, Session, SessionEvent, SessionKeeper, SessionRefresher,
    SessionValidator, StorageError,
};

pub trait Accounts {
    fn current(&self) -> Result<Option<AccountId>, StorageError>;
    fn accounts(&self) -> Result<Vec<Account>, StorageError>;
    fn restore(&self) -> Result<SessionStatus, StorageError>;
    fn sign_in(&self) -> Result<AccountId, SignInError>;
    fn switch(&self, id: &AccountId) -> Result<SessionStatus, StorageError>;
    fn sign_out(&self) -> Result<(), StorageError>;
    fn apply(&self, event: SessionEvent) -> Result<EventOutcome, StorageError>;
}

pub fn sign_out(
    store: &impl AccountStore,
    profiles: &impl ProfileStore,
) -> Result<(), StorageError> {
    let Some(id) = store.active()? else {
        return Ok(());
    };
    match store.delete(&id) {
        Ok(()) | Err(StorageError::NotFound(_)) => {}
        Err(error) => return Err(error),
    }
    store.set_active(None)?;
    profiles
        .remove(&id)
        .map_err(|error| StorageError::Internal(Box::new(error)))
}

pub struct AccountService<P, V, R, F, S, K> {
    provider: P,
    validator: V,
    refresher: R,
    profiles: F,
    store: S,
    keeper: K,
}

impl<P, V, R, F, S, K> AccountService<P, V, R, F, S, K> {
    pub fn new(provider: P, validator: V, refresher: R, profiles: F, store: S, keeper: K) -> Self {
        Self {
            provider,
            validator,
            refresher,
            profiles,
            store,
            keeper,
        }
    }
}

impl<P, V, R, F, S, K> AccountService<P, V, R, F, S, K>
where
    S: AccountStore,
    K: SessionKeeper,
{
    fn resume_keeper(&self) -> Result<(), StorageError> {
        let account = match self.store.active()? {
            Some(id) => self.store.get(&id)?,
            None => None,
        };
        match account {
            Some(account) => self
                .keeper
                .start(&account.id, &Session::new(account.session_cookie.as_str())),
            None => self.keeper.stop(),
        }
        Ok(())
    }
}

impl<P, V, R, F, S, K> Accounts for AccountService<P, V, R, F, S, K>
where
    P: LoginProvider,
    V: SessionValidator,
    R: SessionRefresher,
    F: ProfileStore,
    S: AccountStore,
    K: SessionKeeper,
{
    fn current(&self) -> Result<Option<AccountId>, StorageError> {
        self.store.active()
    }

    fn accounts(&self) -> Result<Vec<Account>, StorageError> {
        self.store.list()
    }

    fn restore(&self) -> Result<SessionStatus, StorageError> {
        let status = restore_session(&self.validator, &self.refresher, &self.store)?;
        match status {
            SessionStatus::SignedIn(_) => self.resume_keeper()?,
            SessionStatus::SignedOut | SessionStatus::Expired(_) => self.keeper.stop(),
        }
        Ok(status)
    }

    fn sign_in(&self) -> Result<AccountId, SignInError> {
        self.keeper.stop();
        let result = sign_in(&self.provider, &self.validator, &self.profiles, &self.store);
        let resumed = self.resume_keeper();
        let id = result?;
        resumed?;
        Ok(id)
    }

    fn switch(&self, id: &AccountId) -> Result<SessionStatus, StorageError> {
        self.keeper.stop();
        let switched = self.store.set_active(Some(id));
        let restored = self.restore();
        switched?;
        restored
    }

    fn sign_out(&self) -> Result<(), StorageError> {
        self.keeper.stop();
        sign_out(&self.store, &self.profiles)
    }

    fn apply(&self, event: SessionEvent) -> Result<EventOutcome, StorageError> {
        let outcome = apply_event(&self.store, event)?;
        if matches!(outcome, EventOutcome::SignedOut(_)) {
            self.keeper.stop();
        }
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::fakes::{
        FakeKeeper, FakeProfiles, FakeProvider, FakeRefresher, FakeStore, FakeValidator,
    };
    use crate::account::Account;
    use crate::{Login, LoginError, RefreshError, ValidationError};

    fn account(id: &str) -> Account {
        Account {
            id: AccountId::new(id),
            session_cookie: "SAPISID=x".into(),
            added_at_unix: 1,
            profile: crate::account::AccountProfile::default(),
        }
    }

    fn login() -> Result<Login, LoginError> {
        Ok(Login {
            session: Session::new("SAPISID=abc"),
            profile: FakeProfiles::staged(),
        })
    }

    fn cancelled() -> Result<Login, LoginError> {
        Err(LoginError::Cancelled)
    }

    fn identity() -> Result<AccountId, ValidationError> {
        Ok(AccountId::new("abc"))
    }

    fn unchanged(_: &AccountId, _: &Session) -> Result<Session, RefreshError> {
        Err(RefreshError::Unchanged)
    }

    fn service_with(
        provider: fn() -> Result<Login, LoginError>,
        store: FakeStore,
        profiles: FakeProfiles,
        keeper: impl SessionKeeper,
    ) -> impl Accounts {
        AccountService::new(
            FakeProvider(provider),
            FakeValidator(identity),
            FakeRefresher(unchanged),
            profiles,
            store,
            keeper,
        )
    }

    fn service(store: FakeStore) -> impl Accounts {
        service_with(login, store, FakeProfiles::default(), ())
    }

    fn save_active(store: &FakeStore, id: &str) {
        store.save(&account(id)).unwrap();
        store.set_active(Some(&AccountId::new(id))).unwrap();
    }

    #[test]
    fn sign_out_removes_the_active_account_its_cookie_and_its_profile() {
        let store = FakeStore::default();
        let profiles = FakeProfiles::default();
        save_active(&store, "a");

        sign_out(&store, &profiles).unwrap();

        assert_eq!(store.get(&AccountId::new("a")).unwrap(), None);
        assert_eq!(store.active().unwrap(), None);
        assert_eq!(profiles.events(), ["remove a"]);
    }

    #[test]
    fn sign_out_leaves_other_saved_accounts_and_their_profiles_alone() {
        let store = FakeStore::default();
        let profiles = FakeProfiles::default();
        store.save(&account("b")).unwrap();
        save_active(&store, "a");

        sign_out(&store, &profiles).unwrap();

        assert!(store.get(&AccountId::new("b")).unwrap().is_some());
        assert_eq!(profiles.events(), ["remove a"]);
    }

    #[test]
    fn sign_out_while_signed_out_is_a_no_op() {
        let store = FakeStore::default();
        let profiles = FakeProfiles::default();
        store.save(&account("a")).unwrap();

        sign_out(&store, &profiles).unwrap();

        assert!(store.get(&AccountId::new("a")).unwrap().is_some());
        assert!(profiles.events().is_empty());
    }

    #[test]
    fn sign_out_clears_an_active_id_whose_account_is_already_gone() {
        let store = FakeStore::default();
        let profiles = FakeProfiles::default();
        store.set_active_unchecked(Some(AccountId::new("ghost")));

        sign_out(&store, &profiles).unwrap();

        assert_eq!(store.active().unwrap(), None);
        assert_eq!(profiles.events(), ["remove ghost"]);
    }

    #[test]
    fn current_is_none_until_signed_in_then_survives_as_the_active_account() {
        let accounts = service(FakeStore::default());
        assert_eq!(accounts.current().unwrap(), None);

        let id = accounts.sign_in().unwrap();

        assert_eq!(accounts.current().unwrap(), Some(id));
    }

    #[test]
    fn restore_after_sign_in_reports_the_account_signed_in() {
        let accounts = service(FakeStore::default());
        let id = accounts.sign_in().unwrap();

        assert_eq!(accounts.restore().unwrap(), SessionStatus::SignedIn(id));
    }

    #[test]
    fn current_is_none_again_after_sign_out() {
        let accounts = service(FakeStore::default());
        accounts.sign_in().unwrap();

        accounts.sign_out().unwrap();

        assert_eq!(accounts.current().unwrap(), None);
    }

    #[test]
    fn a_failed_sign_in_does_not_change_the_current_account() {
        let accounts = service_with(cancelled, FakeStore::default(), FakeProfiles::default(), ());

        assert!(accounts.sign_in().is_err());
        assert_eq!(accounts.current().unwrap(), None);
    }

    #[test]
    fn restoring_a_signed_in_session_starts_the_keeper_for_that_account() {
        let keeper = FakeKeeper::default();
        let store = FakeStore::default();
        save_active(&store, "abc");
        let accounts = service_with(login, store, FakeProfiles::default(), keeper.clone());

        accounts.restore().unwrap();

        assert_eq!(keeper.events(), ["start abc SAPISID=x"]);
    }

    #[test]
    fn restoring_with_nobody_signed_in_stops_the_keeper() {
        let keeper = FakeKeeper::default();
        let accounts = service_with(
            login,
            FakeStore::default(),
            FakeProfiles::default(),
            keeper.clone(),
        );

        accounts.restore().unwrap();

        assert_eq!(keeper.events(), ["stop"]);
    }

    #[test]
    fn signing_in_pauses_the_keeper_then_keeps_the_new_account_alive() {
        let keeper = FakeKeeper::default();
        let accounts = service_with(
            login,
            FakeStore::default(),
            FakeProfiles::default(),
            keeper.clone(),
        );

        accounts.sign_in().unwrap();

        assert_eq!(keeper.events(), ["stop", "start abc SAPISID=abc"]);
    }

    #[test]
    fn a_failed_sign_in_resumes_the_previous_account() {
        let keeper = FakeKeeper::default();
        let store = FakeStore::default();
        save_active(&store, "a");
        let accounts = service_with(cancelled, store, FakeProfiles::default(), keeper.clone());

        assert!(accounts.sign_in().is_err());

        assert_eq!(keeper.events(), ["stop", "start a SAPISID=x"]);
    }

    #[test]
    fn signing_out_stops_the_keeper_and_removes_the_profile() {
        let keeper = FakeKeeper::default();
        let profiles = FakeProfiles::default();
        let store = FakeStore::default();
        save_active(&store, "a");
        let accounts = service_with(login, store, profiles.clone(), keeper.clone());

        accounts.sign_out().unwrap();

        assert_eq!(keeper.events(), ["stop"]);
        assert_eq!(profiles.events(), ["remove a"]);
    }

    #[test]
    fn a_keeper_sign_out_of_the_active_account_stops_the_keeper() {
        let keeper = FakeKeeper::default();
        let store = FakeStore::default();
        save_active(&store, "a");
        let accounts = service_with(login, store, FakeProfiles::default(), keeper.clone());

        let outcome = accounts
            .apply(SessionEvent::SignedOut {
                account: AccountId::new("a"),
            })
            .unwrap();

        assert_eq!(outcome, EventOutcome::SignedOut(AccountId::new("a")));
        assert_eq!(keeper.events(), ["stop"]);
    }

    #[test]
    fn a_refreshed_session_is_saved_and_leaves_the_keeper_running() {
        let keeper = FakeKeeper::default();
        let store = FakeStore::default();
        save_active(&store, "a");
        let accounts = service_with(login, store, FakeProfiles::default(), keeper.clone());

        let outcome = accounts
            .apply(SessionEvent::Refreshed {
                account: AccountId::new("a"),
                session: Session::new("SAPISID=x; TS=new"),
            })
            .unwrap();

        assert_eq!(outcome, EventOutcome::Saved);
        assert!(keeper.events().is_empty());
    }

    #[test]
    fn accounts_lists_everything_that_is_saved() {
        let store = FakeStore::default();
        store.save(&account("a")).unwrap();
        store.save(&account("b")).unwrap();
        let accounts = service(store);

        let ids: Vec<_> = accounts
            .accounts()
            .unwrap()
            .into_iter()
            .map(|account| account.id)
            .collect();

        assert_eq!(ids, [AccountId::new("a"), AccountId::new("b")]);
    }

    fn two_accounts() -> FakeStore {
        let store = FakeStore::default();
        store.save(&account("abc")).unwrap();
        store.save(&account("b")).unwrap();
        store.set_active(Some(&AccountId::new("b"))).unwrap();
        store
    }

    #[test]
    fn switching_activates_the_account_and_keeps_it_alive_instead_of_the_old_one() {
        let keeper = FakeKeeper::default();
        let accounts = service_with(
            login,
            two_accounts(),
            FakeProfiles::default(),
            keeper.clone(),
        );

        let status = accounts.switch(&AccountId::new("abc")).unwrap();

        assert_eq!(status, SessionStatus::SignedIn(AccountId::new("abc")));
        assert_eq!(accounts.current().unwrap(), Some(AccountId::new("abc")));
        assert_eq!(keeper.events(), ["stop", "start abc SAPISID=x"]);
    }

    #[test]
    fn switching_to_an_unknown_account_changes_nothing() {
        let keeper = FakeKeeper::default();
        let accounts = service_with(
            login,
            two_accounts(),
            FakeProfiles::default(),
            keeper.clone(),
        );

        let result = accounts.switch(&AccountId::new("ghost"));

        assert!(matches!(result, Err(StorageError::NotFound(_))));
        assert_eq!(accounts.current().unwrap(), Some(AccountId::new("b")));
    }
}
