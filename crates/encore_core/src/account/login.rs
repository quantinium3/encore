use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use thiserror::Error;

use super::{Account, AccountId, AccountStore};
use crate::{
    Login, LoginError, LoginProvider, ProfileStore, Session, SessionValidator, StorageError,
    ValidationError, VerifiedSession,
};

#[derive(Debug, Error)]
pub enum SignInError {
    #[error(transparent)]
    Login(#[from] LoginError),
    #[error("YouTube rejected the session; sign in again")]
    Rejected,
    #[error("could not check the session with YouTube: {0}")]
    Validation(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("could not save the login profile: {0}")]
    Profile(#[source] io::Error),
    #[error(transparent)]
    Storage(#[from] StorageError),
}

pub fn sign_in(
    provider: &impl LoginProvider,
    validator: &impl SessionValidator,
    profiles: &impl ProfileStore,
    store: &impl AccountStore,
) -> Result<AccountId, SignInError> {
    let Login { session, profile } = provider.login()?;
    let result = register(validator, profiles, store, &session, &profile);
    if result.is_err() {
        profiles.discard(&profile);
    }
    result
}

fn register(
    validator: &impl SessionValidator,
    profiles: &impl ProfileStore,
    store: &impl AccountStore,
    session: &Session,
    profile_dir: &Path,
) -> Result<AccountId, SignInError> {
    let VerifiedSession { id, profile } =
        validator.validate(session).map_err(|error| match error {
            ValidationError::Rejected => SignInError::Rejected,
            ValidationError::Backend(source) => SignInError::Validation(source),
        })?;
    let existing = store.get(&id)?;
    let existed = existing.is_some();
    let profile = match existing {
        Some(account) => account.profile.merged_with(profile),
        None => profile,
    };

    profiles
        .adopt(profile_dir, &id)
        .map_err(SignInError::Profile)?;
    let saved = store
        .save(&Account {
            id: id.clone(),
            session_cookie: session.cookie().to_owned(),
            added_at_unix: now_unix(),
            profile,
        })
        .and_then(|()| store.set_active(Some(&id)));
    if let Err(error) = saved {
        if !existed {
            let _ = profiles.remove(&id);
        }
        return Err(error.into());
    }
    Ok(id)
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::AccountProfile;
    use crate::account::fakes::{
        FakeProfiles, FakeProvider, FakeStore, FakeValidator, VerifiedValidator,
    };

    fn good_login() -> Result<Login, LoginError> {
        Ok(Login {
            session: Session::new("SAPISID=abc"),
            profile: FakeProfiles::staged(),
        })
    }

    fn alice() -> Result<AccountId, ValidationError> {
        Ok(AccountId::new("alice"))
    }

    #[test]
    fn a_valid_login_is_adopted_saved_and_made_active() {
        let store = FakeStore::default();
        let profiles = FakeProfiles::default();

        let id = sign_in(
            &FakeProvider(good_login),
            &FakeValidator(alice),
            &profiles,
            &store,
        )
        .unwrap();

        let saved = store.get(&id).unwrap().unwrap();
        assert_eq!(id.as_str(), "alice");
        assert_eq!(saved.session_cookie, "SAPISID=abc");
        assert!(saved.added_at_unix > 0);
        assert_eq!(store.active().unwrap(), Some(id));
        assert_eq!(profiles.events(), ["adopt staged alice"]);
    }

    #[test]
    fn signing_in_again_keeps_the_original_added_time_and_replaces_the_profile() {
        let store = FakeStore::default();
        store
            .save(&Account {
                id: AccountId::new("alice"),
                session_cookie: "SAPISID=old".into(),
                added_at_unix: 7,
                profile: AccountProfile {
                    name: "Old Name".into(),
                    detail: Some("@old".into()),
                    avatar_url: None,
                },
            })
            .unwrap();
        let profiles = FakeProfiles::default();

        sign_in(
            &FakeProvider(good_login),
            &FakeValidator(alice),
            &profiles,
            &store,
        )
        .unwrap();

        let saved = store.get(&AccountId::new("alice")).unwrap().unwrap();
        assert_eq!(saved.session_cookie, "SAPISID=abc");
        assert_eq!(saved.added_at_unix, 7);
        assert_eq!(store.list().unwrap().len(), 1);
    }

    #[test]
    fn a_cancelled_login_saves_nothing_and_has_no_profile_to_clean_up() {
        let store = FakeStore::default();
        let profiles = FakeProfiles::default();
        let provider = FakeProvider(|| Err(LoginError::Cancelled));

        let result = sign_in(&provider, &FakeValidator(alice), &profiles, &store);

        assert!(matches!(
            result,
            Err(SignInError::Login(LoginError::Cancelled))
        ));
        assert!(store.list().unwrap().is_empty());
        assert!(profiles.events().is_empty());
    }

    #[test]
    fn a_rejected_session_saves_nothing_and_discards_the_profile() {
        let store = FakeStore::default();
        let profiles = FakeProfiles::default();
        let validator = FakeValidator(|| Err(ValidationError::Rejected));

        let result = sign_in(&FakeProvider(good_login), &validator, &profiles, &store);

        assert!(matches!(result, Err(SignInError::Rejected)));
        assert!(store.list().unwrap().is_empty());
        assert_eq!(store.active().unwrap(), None);
        assert_eq!(profiles.events(), ["discard staged"]);
    }

    #[test]
    fn a_validation_backend_failure_saves_nothing_and_discards_the_profile() {
        let store = FakeStore::default();
        let profiles = FakeProfiles::default();
        let validator =
            FakeValidator(|| Err(ValidationError::Backend("offline".to_owned().into())));

        let result = sign_in(&FakeProvider(good_login), &validator, &profiles, &store);

        assert!(matches!(result, Err(SignInError::Validation(_))));
        assert!(store.list().unwrap().is_empty());
        assert_eq!(profiles.events(), ["discard staged"]);
    }

    #[test]
    fn a_profile_that_cannot_be_adopted_saves_nothing() {
        let store = FakeStore::default();
        let profiles = FakeProfiles::failing_adopt();

        let result = sign_in(
            &FakeProvider(good_login),
            &FakeValidator(alice),
            &profiles,
            &store,
        );

        assert!(matches!(result, Err(SignInError::Profile(_))));
        assert!(store.list().unwrap().is_empty());
        assert_eq!(store.active().unwrap(), None);
    }

    #[test]
    fn a_failed_save_of_a_new_account_removes_the_profile_it_just_adopted() {
        let store = FakeStore::failing_save();
        let profiles = FakeProfiles::default();

        let result = sign_in(
            &FakeProvider(good_login),
            &FakeValidator(alice),
            &profiles,
            &store,
        );

        assert!(matches!(result, Err(SignInError::Storage(_))));
        assert_eq!(
            profiles.events(),
            ["adopt staged alice", "remove alice", "discard staged"]
        );
    }

    fn verified() -> Result<VerifiedSession, ValidationError> {
        Ok(VerifiedSession {
            id: AccountId::new("alice"),
            profile: AccountProfile {
                name: "Alice".into(),
                detail: Some("@alice".into()),
                avatar_url: Some("https://img.example/alice.jpg".into()),
            },
        })
    }

    #[test]
    fn the_verified_profile_is_saved_with_the_account() {
        let store = FakeStore::default();

        let id = sign_in(
            &FakeProvider(good_login),
            &VerifiedValidator(verified),
            &FakeProfiles::default(),
            &store,
        )
        .unwrap();

        let saved = store.get(&id).unwrap().unwrap();
        assert_eq!(saved.profile.name, "Alice");
        assert_eq!(saved.profile.detail.as_deref(), Some("@alice"));
        assert_eq!(
            saved.profile.avatar_url.as_deref(),
            Some("https://img.example/alice.jpg")
        );
    }

    #[test]
    fn signing_in_again_without_profile_details_keeps_the_stored_ones() {
        let store = FakeStore::default();
        store
            .save(&Account {
                id: AccountId::new("alice"),
                session_cookie: "SAPISID=old".into(),
                added_at_unix: 7,
                profile: AccountProfile {
                    name: "Old Name".into(),
                    detail: Some("@old".into()),
                    avatar_url: Some("old.jpg".into()),
                },
            })
            .unwrap();

        sign_in(
            &FakeProvider(good_login),
            &FakeValidator(alice),
            &FakeProfiles::default(),
            &store,
        )
        .unwrap();

        let saved = store.get(&AccountId::new("alice")).unwrap().unwrap();
        assert_eq!(saved.profile.name, "Old Name");
        assert_eq!(saved.profile.avatar_url.as_deref(), Some("old.jpg"));
    }
}
