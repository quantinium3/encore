use super::{Account, AccountId, AccountProfile, AccountStore};
use crate::{
    RefreshError, Session, SessionRefresher, SessionValidator, StorageError, ValidationError,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionStatus {
    SignedOut,
    SignedIn(AccountId),
    Expired(AccountId),
}

pub fn restore_session(
    validator: &impl SessionValidator,
    refresher: &impl SessionRefresher,
    store: &impl AccountStore,
) -> Result<SessionStatus, StorageError> {
    let Some(id) = store.active()? else {
        return Ok(SessionStatus::SignedOut);
    };
    let Some(account) = store.get(&id)? else {
        store.set_active(None)?;
        return Ok(SessionStatus::SignedOut);
    };

    let stored = Session::new(account.session_cookie.as_str());
    match validator.validate(&stored) {
        Ok(found) if found.id == id => {
            refresh_profile(store, &account, found.profile)?;
            return Ok(SessionStatus::SignedIn(id));
        }
        Ok(_) => return Ok(SessionStatus::Expired(id)),
        Err(ValidationError::Backend(_)) => return Ok(SessionStatus::SignedIn(id)),
        Err(ValidationError::Rejected) => {}
    }

    let fresh = match refresher.refresh(&id, &stored) {
        Ok(fresh) => fresh,
        Err(RefreshError::NotSignedIn | RefreshError::Unchanged | RefreshError::Backend(_)) => {
            return Ok(SessionStatus::Expired(id));
        }
    };
    let profile = match validator.validate(&fresh) {
        Ok(found) if found.id == id => found.profile,
        Ok(_) | Err(ValidationError::Rejected) => return Ok(SessionStatus::Expired(id)),
        Err(ValidationError::Backend(_)) => AccountProfile::default(),
    };

    store.save(&Account {
        id: id.clone(),
        session_cookie: fresh.into_cookie(),
        added_at_unix: account.added_at_unix,
        profile: account.profile.merged_with(profile),
    })?;
    Ok(SessionStatus::SignedIn(id))
}

fn refresh_profile(
    store: &impl AccountStore,
    account: &Account,
    newer: AccountProfile,
) -> Result<(), StorageError> {
    let merged = account.profile.clone().merged_with(newer);
    if merged == account.profile {
        return Ok(());
    }
    store.save(&Account {
        profile: merged,
        ..account.clone()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VerifiedSession;
    use crate::account::fakes::{
        CookieValidator, CookieVerifiedValidator, FakeRefresher, FakeStore,
    };

    type Refresh = fn(&AccountId, &Session) -> Result<Session, RefreshError>;

    fn abc() -> AccountId {
        AccountId::new("abc")
    }

    fn accepts_only_fresh(cookie: &str) -> Result<AccountId, ValidationError> {
        match cookie.contains("TS=fresh") {
            true => Ok(abc()),
            false => Err(ValidationError::Rejected),
        }
    }

    fn signed_in_store(cookie: &str) -> FakeStore {
        let store = FakeStore::default();
        store
            .save(&Account {
                id: abc(),
                session_cookie: cookie.into(),
                added_at_unix: 7,
                profile: AccountProfile::default(),
            })
            .unwrap();
        store.set_active(Some(&abc())).unwrap();
        store
    }

    fn stored_cookie(store: &FakeStore) -> String {
        store.get(&abc()).unwrap().unwrap().session_cookie
    }

    fn no_refresh(_: &AccountId, _: &Session) -> Result<Session, RefreshError> {
        panic!("the refresher must not be called")
    }

    fn fresh_same_account(_: &AccountId, _: &Session) -> Result<Session, RefreshError> {
        Ok(Session::new("SAPISID=abc; TS=fresh"))
    }

    fn restore(
        validator: fn(&str) -> Result<AccountId, ValidationError>,
        refresh: Refresh,
        store: &FakeStore,
    ) -> SessionStatus {
        restore_session(&CookieValidator(validator), &FakeRefresher(refresh), store).unwrap()
    }

    #[test]
    fn nobody_signed_in_is_signed_out() {
        let store = FakeStore::default();

        let status = restore(accepts_only_fresh, no_refresh, &store);

        assert_eq!(status, SessionStatus::SignedOut);
    }

    #[test]
    fn an_active_id_with_no_saved_account_is_cleared() {
        let store = FakeStore::default();
        store.set_active_unchecked(Some(AccountId::new("ghost")));

        let status = restore(accepts_only_fresh, no_refresh, &store);

        assert_eq!(status, SessionStatus::SignedOut);
        assert_eq!(store.active().unwrap(), None);
    }

    #[test]
    fn a_still_valid_session_is_left_alone() {
        let store = signed_in_store("SAPISID=abc; TS=fresh");

        let status = restore(accepts_only_fresh, no_refresh, &store);

        assert_eq!(status, SessionStatus::SignedIn(abc()));
        assert_eq!(stored_cookie(&store), "SAPISID=abc; TS=fresh");
    }

    #[test]
    fn a_session_that_belongs_to_another_identity_expires_the_account() {
        let store = signed_in_store("SAPISID=abc; TS=fresh");

        let status = restore(|_| Ok(AccountId::new("someone-else")), no_refresh, &store);

        assert_eq!(status, SessionStatus::Expired(abc()));
        assert_eq!(stored_cookie(&store), "SAPISID=abc; TS=fresh");
    }

    #[test]
    fn a_network_failure_does_not_sign_the_user_out() {
        let store = signed_in_store("SAPISID=abc; TS=old");

        let status = restore(
            |_| Err(ValidationError::Backend("offline".to_owned().into())),
            no_refresh,
            &store,
        );

        assert_eq!(status, SessionStatus::SignedIn(abc()));
        assert_eq!(stored_cookie(&store), "SAPISID=abc; TS=old");
    }

    #[test]
    fn a_rejected_session_is_healed_with_a_fresh_one_for_the_same_account() {
        let store = signed_in_store("SAPISID=abc; TS=old");

        let status = restore(accepts_only_fresh, fresh_same_account, &store);

        assert_eq!(status, SessionStatus::SignedIn(abc()));
        assert_eq!(stored_cookie(&store), "SAPISID=abc; TS=fresh");
        assert_eq!(store.get(&abc()).unwrap().unwrap().added_at_unix, 7);
    }

    #[test]
    fn the_refresher_is_asked_about_the_active_account() {
        let store = signed_in_store("SAPISID=abc; TS=old");

        let status = restore(
            accepts_only_fresh,
            |account, _| {
                assert_eq!(account, &AccountId::new("abc"));
                Ok(Session::new("SAPISID=abc; TS=fresh"))
            },
            &store,
        );

        assert_eq!(status, SessionStatus::SignedIn(abc()));
    }

    #[test]
    fn a_fresh_session_for_a_different_identity_is_not_used() {
        let store = signed_in_store("SAPISID=abc; TS=old");

        let status = restore(
            |cookie| match cookie.contains("TS=fresh") {
                true => Ok(AccountId::new("someone-else")),
                false => Err(ValidationError::Rejected),
            },
            fresh_same_account,
            &store,
        );

        assert_eq!(status, SessionStatus::Expired(abc()));
        assert_eq!(stored_cookie(&store), "SAPISID=abc; TS=old");
    }

    #[test]
    fn a_fresh_session_that_is_also_rejected_expires_the_account() {
        let store = signed_in_store("SAPISID=abc; TS=old");

        let status = restore(
            accepts_only_fresh,
            |_, _| Ok(Session::new("SAPISID=abc; TS=also-old")),
            &store,
        );

        assert_eq!(status, SessionStatus::Expired(abc()));
        assert_eq!(stored_cookie(&store), "SAPISID=abc; TS=old");
    }

    #[test]
    fn a_network_failure_while_checking_the_fresh_session_keeps_it() {
        let store = signed_in_store("SAPISID=abc; TS=old");

        let status = restore(
            |cookie| match cookie.contains("TS=fresh") {
                true => Err(ValidationError::Backend("offline".to_owned().into())),
                false => Err(ValidationError::Rejected),
            },
            fresh_same_account,
            &store,
        );

        assert_eq!(status, SessionStatus::SignedIn(abc()));
        assert_eq!(stored_cookie(&store), "SAPISID=abc; TS=fresh");
    }

    #[test]
    fn a_refresh_that_offers_nothing_expires_the_account() {
        let store = signed_in_store("SAPISID=abc; TS=old");
        let cases: [Refresh; 3] = [
            |_, _| Err(RefreshError::Unchanged),
            |_, _| Err(RefreshError::NotSignedIn),
            |_, _| Err(RefreshError::Backend("helper crashed".to_owned().into())),
        ];

        for refresh in cases {
            let status = restore(accepts_only_fresh, refresh, &store);

            assert_eq!(status, SessionStatus::Expired(abc()));
        }

        assert_eq!(stored_cookie(&store), "SAPISID=abc; TS=old");
    }

    fn named(name: &str) -> AccountProfile {
        AccountProfile {
            name: name.to_owned(),
            detail: Some("@handle".to_owned()),
            avatar_url: Some("https://img.example/a.jpg".to_owned()),
        }
    }

    fn restore_verified(
        validator: fn(&str) -> Result<VerifiedSession, ValidationError>,
        refresh: Refresh,
        store: &FakeStore,
    ) -> SessionStatus {
        restore_session(
            &CookieVerifiedValidator(validator),
            &FakeRefresher(refresh),
            store,
        )
        .unwrap()
    }

    fn stored_profile(store: &FakeStore) -> AccountProfile {
        store.get(&abc()).unwrap().unwrap().profile
    }

    #[test]
    fn a_valid_session_refreshes_the_stored_profile() {
        let store = signed_in_store("SAPISID=abc; TS=fresh");

        let status = restore_verified(
            |_| {
                Ok(VerifiedSession {
                    id: abc(),
                    profile: named("Alice"),
                })
            },
            no_refresh,
            &store,
        );

        assert_eq!(status, SessionStatus::SignedIn(abc()));
        assert_eq!(stored_profile(&store), named("Alice"));
        assert_eq!(stored(&store).session_cookie, "SAPISID=abc; TS=fresh");
    }

    #[test]
    fn a_response_without_profile_details_keeps_the_stored_ones() {
        let store = signed_in_store("SAPISID=abc; TS=fresh");
        let mut account = store.get(&abc()).unwrap().unwrap();
        account.profile = named("Alice");
        store.save(&account).unwrap();

        restore_verified(|_| Ok(VerifiedSession::new(abc())), no_refresh, &store);

        assert_eq!(stored_profile(&store), named("Alice"));
    }

    #[test]
    fn a_healed_session_stores_the_fresh_profile_too() {
        let store = signed_in_store("SAPISID=abc; TS=old");

        let status = restore_verified(
            |cookie| match cookie.contains("TS=fresh") {
                true => Ok(VerifiedSession {
                    id: abc(),
                    profile: named("Alice"),
                }),
                false => Err(ValidationError::Rejected),
            },
            fresh_same_account,
            &store,
        );

        assert_eq!(status, SessionStatus::SignedIn(abc()));
        assert_eq!(stored_profile(&store), named("Alice"));
        assert_eq!(stored(&store).session_cookie, "SAPISID=abc; TS=fresh");
    }

    fn stored(store: &FakeStore) -> Account {
        store.get(&abc()).unwrap().unwrap()
    }
}
