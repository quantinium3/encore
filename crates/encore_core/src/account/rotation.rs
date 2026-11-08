use super::{Account, AccountId, AccountStore};
use crate::{SessionEvent, StorageError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventOutcome {
    Ignored,
    Saved,
    SignedOut(AccountId),
}

pub fn apply_event(
    store: &impl AccountStore,
    event: SessionEvent,
) -> Result<EventOutcome, StorageError> {
    let account = match &event {
        SessionEvent::Refreshed { account, .. } | SessionEvent::SignedOut { account } => account,
    };
    if store.active()?.as_ref() != Some(account) {
        return Ok(EventOutcome::Ignored);
    }

    match event {
        SessionEvent::SignedOut { account } => Ok(EventOutcome::SignedOut(account)),
        SessionEvent::Refreshed { account, session } => {
            let Some(saved) = store.get(&account)? else {
                return Ok(EventOutcome::Ignored);
            };
            if saved.session_cookie == session.cookie() {
                return Ok(EventOutcome::Ignored);
            }
            store.save(&Account {
                id: account,
                session_cookie: session.into_cookie(),
                added_at_unix: saved.added_at_unix,
                profile: saved.profile,
            })?;
            Ok(EventOutcome::Saved)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;
    use crate::account::AccountProfile;
    use crate::account::fakes::FakeStore;

    fn abc() -> AccountId {
        AccountId::new("abc")
    }

    fn refreshed(account: &str, cookie: &str) -> SessionEvent {
        SessionEvent::Refreshed {
            account: AccountId::new(account),
            session: Session::new(cookie),
        }
    }

    fn signed_in_store() -> FakeStore {
        let store = FakeStore::default();
        store
            .save(&Account {
                id: abc(),
                session_cookie: "SAPISID=abc; TS=old".into(),
                added_at_unix: 7,
                profile: AccountProfile::default(),
            })
            .unwrap();
        store.set_active(Some(&abc())).unwrap();
        store
    }

    fn stored(store: &FakeStore) -> Account {
        store.get(&abc()).unwrap().unwrap()
    }

    #[test]
    fn a_rotated_cookie_replaces_the_stored_one_and_keeps_the_rest() {
        let store = signed_in_store();

        let outcome = apply_event(&store, refreshed("abc", "SAPISID=abc; TS=new")).unwrap();

        assert_eq!(outcome, EventOutcome::Saved);
        assert_eq!(stored(&store).session_cookie, "SAPISID=abc; TS=new");
        assert_eq!(stored(&store).added_at_unix, 7);
    }

    #[test]
    fn an_unchanged_cookie_is_not_written() {
        let store = signed_in_store();

        let outcome = apply_event(&store, refreshed("abc", "SAPISID=abc; TS=old")).unwrap();

        assert_eq!(outcome, EventOutcome::Ignored);
    }

    #[test]
    fn a_session_for_an_account_that_is_not_active_is_ignored() {
        let store = signed_in_store();
        store
            .save(&Account {
                id: AccountId::new("other"),
                session_cookie: "SAPISID=other".into(),
                added_at_unix: 1,
                profile: AccountProfile::default(),
            })
            .unwrap();

        let outcome = apply_event(&store, refreshed("other", "SAPISID=other; TS=new")).unwrap();

        assert_eq!(outcome, EventOutcome::Ignored);
        assert_eq!(
            store
                .get(&AccountId::new("other"))
                .unwrap()
                .unwrap()
                .session_cookie,
            "SAPISID=other"
        );
    }

    #[test]
    fn nothing_is_saved_while_signed_out() {
        let store = signed_in_store();
        store.set_active(None).unwrap();

        let outcome = apply_event(&store, refreshed("abc", "SAPISID=abc; TS=new")).unwrap();

        assert_eq!(outcome, EventOutcome::Ignored);
        assert_eq!(stored(&store).session_cookie, "SAPISID=abc; TS=old");
    }

    #[test]
    fn a_sign_out_of_the_active_account_is_reported_and_keeps_the_account() {
        let store = signed_in_store();

        let outcome = apply_event(&store, SessionEvent::SignedOut { account: abc() }).unwrap();

        assert_eq!(outcome, EventOutcome::SignedOut(abc()));
        assert!(store.get(&abc()).unwrap().is_some());
        assert_eq!(store.active().unwrap(), Some(abc()));
    }

    #[test]
    fn a_sign_out_of_an_account_that_is_not_active_is_ignored() {
        let store = signed_in_store();

        let outcome = apply_event(
            &store,
            SessionEvent::SignedOut {
                account: AccountId::new("other"),
            },
        )
        .unwrap();

        assert_eq!(outcome, EventOutcome::Ignored);
    }

    #[test]
    fn a_rotated_cookie_keeps_the_stored_profile() {
        let store = signed_in_store();
        let mut account = stored(&store);
        account.profile = AccountProfile {
            name: "Abc".into(),
            detail: None,
            avatar_url: Some("abc.jpg".into()),
        };
        store.save(&account).unwrap();

        apply_event(&store, refreshed("abc", "SAPISID=abc; TS=new")).unwrap();

        assert_eq!(stored(&store).profile, account.profile);
    }
}
