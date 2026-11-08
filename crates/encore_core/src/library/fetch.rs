use thiserror::Error;

use super::{Playlist, PlaylistDetail, PlaylistError, PlaylistProvider};
use crate::{AccountStore, Session, StorageError};

#[derive(Debug, Error)]
pub enum LibraryError {
    #[error("nobody is signed in")]
    SignedOut,
    #[error(transparent)]
    Provider(#[from] PlaylistError),
    #[error(transparent)]
    Storage(#[from] StorageError),
}

pub fn fetch_playlists(
    store: &impl AccountStore,
    provider: &impl PlaylistProvider,
) -> Result<Vec<Playlist>, LibraryError> {
    let Some(id) = store.active()? else {
        return Err(LibraryError::SignedOut);
    };
    let Some(account) = store.get(&id)? else {
        return Err(LibraryError::SignedOut);
    };
    let playlists = provider.playlists(&Session::new(account.session_cookie))?;
    Ok(playlists)
}

pub fn fetch_playlist_detail(
    store: &impl AccountStore,
    provider: &impl PlaylistProvider,
    playlist_id: &str,
) -> Result<PlaylistDetail, LibraryError> {
    let Some(id) = store.active()? else {
        return Err(LibraryError::SignedOut);
    };
    let Some(account) = store.get(&id)? else {
        return Err(LibraryError::SignedOut);
    };
    let detail = provider.playlist(&Session::new(account.session_cookie), playlist_id)?;
    Ok(detail)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::account::fakes::FakeStore;
    use crate::{Account, AccountId, AccountProfile};

    fn account(id: &str) -> Account {
        Account {
            id: AccountId::new(id),
            session_cookie: format!("SAPISID={id}"),
            added_at_unix: 1,
            profile: AccountProfile::default(),
        }
    }

    fn playlist(id: &str) -> Playlist {
        Playlist {
            id: id.to_owned(),
            title: id.to_owned(),
            track_count: None,
            thumbnail_url: None,
            owner: None,
        }
    }

    fn detail(title: &str) -> PlaylistDetail {
        PlaylistDetail {
            title: title.to_owned(),
            description: None,
            thumbnail_url: None,
            track_count: None,
            year: None,
            total_duration: None,
            privacy: None,
            tracks: Vec::new(),
        }
    }

    struct FakeProvider {
        cookies: RefCell<Vec<String>>,
        playlists: fn(&str) -> Result<Vec<Playlist>, PlaylistError>,
        playlist: fn(&str, &str) -> Result<PlaylistDetail, PlaylistError>,
    }

    impl FakeProvider {
        fn new(playlists: fn(&str) -> Result<Vec<Playlist>, PlaylistError>) -> Self {
            Self {
                cookies: RefCell::new(Vec::new()),
                playlists,
                playlist: |_, _| panic!("playlist() should not be called in this test"),
            }
        }

        fn for_playlist(playlist: fn(&str, &str) -> Result<PlaylistDetail, PlaylistError>) -> Self {
            Self {
                cookies: RefCell::new(Vec::new()),
                playlists: |_| panic!("playlists() should not be called in this test"),
                playlist,
            }
        }

        fn cookies_seen(&self) -> Vec<String> {
            self.cookies.borrow().clone()
        }
    }

    impl PlaylistProvider for FakeProvider {
        fn playlists(&self, session: &Session) -> Result<Vec<Playlist>, PlaylistError> {
            self.cookies.borrow_mut().push(session.cookie().to_owned());
            (self.playlists)(session.cookie())
        }

        fn playlist(
            &self,
            session: &Session,
            playlist_id: &str,
        ) -> Result<PlaylistDetail, PlaylistError> {
            self.cookies.borrow_mut().push(session.cookie().to_owned());
            (self.playlist)(session.cookie(), playlist_id)
        }
    }

    #[test]
    fn nobody_signed_in_is_an_error_and_the_provider_is_never_asked() {
        let store = FakeStore::default();
        let provider = FakeProvider::new(|_| Ok(vec![playlist("a")]));

        let result = fetch_playlists(&store, &provider);

        assert!(matches!(result, Err(LibraryError::SignedOut)));
        assert!(provider.cookies_seen().is_empty());
    }

    #[test]
    fn an_active_id_whose_account_is_gone_is_signed_out() {
        let store = FakeStore::default();
        store.set_active_unchecked(Some(AccountId::new("ghost")));
        let provider = FakeProvider::new(|_| Ok(vec![playlist("a")]));

        let result = fetch_playlists(&store, &provider);

        assert!(matches!(result, Err(LibraryError::SignedOut)));
    }

    #[test]
    fn the_active_accounts_session_cookie_is_handed_to_the_provider() {
        let store = FakeStore::default();
        store.save(&account("abc")).unwrap();
        store.set_active(Some(&AccountId::new("abc"))).unwrap();
        let provider = FakeProvider::new(|_| Ok(vec![playlist("a"), playlist("b")]));

        let playlists = fetch_playlists(&store, &provider).unwrap();

        assert_eq!(provider.cookies_seen(), ["SAPISID=abc"]);
        assert_eq!(
            playlists.into_iter().map(|p| p.id).collect::<Vec<_>>(),
            ["a", "b"]
        );
    }

    #[test]
    fn a_provider_failure_is_reported_not_hidden() {
        let store = FakeStore::default();
        store.save(&account("abc")).unwrap();
        store.set_active(Some(&AccountId::new("abc"))).unwrap();
        let provider = FakeProvider::new(|_| Err(PlaylistError::Rejected));

        let result = fetch_playlists(&store, &provider);

        assert!(matches!(
            result,
            Err(LibraryError::Provider(PlaylistError::Rejected))
        ));
    }

    #[test]
    fn nobody_signed_in_means_the_detail_provider_is_never_asked() {
        let store = FakeStore::default();
        let provider = FakeProvider::for_playlist(|_, _| Ok(detail("a")));

        let result = fetch_playlist_detail(&store, &provider, "VLabc");

        assert!(matches!(result, Err(LibraryError::SignedOut)));
        assert!(provider.cookies_seen().is_empty());
    }

    #[test]
    fn the_playlist_id_and_the_active_accounts_cookie_reach_the_provider() {
        let store = FakeStore::default();
        store.save(&account("abc")).unwrap();
        store.set_active(Some(&AccountId::new("abc"))).unwrap();
        let provider = FakeProvider::for_playlist(|_, id| Ok(detail(id)));

        let found = fetch_playlist_detail(&store, &provider, "VLabc").unwrap();

        assert_eq!(provider.cookies_seen(), ["SAPISID=abc"]);
        assert_eq!(found.title, "VLabc");
    }

    #[test]
    fn a_detail_provider_failure_is_reported_not_hidden() {
        let store = FakeStore::default();
        store.save(&account("abc")).unwrap();
        store.set_active(Some(&AccountId::new("abc"))).unwrap();
        let provider = FakeProvider::for_playlist(|_, _| Err(PlaylistError::Rejected));

        let result = fetch_playlist_detail(&store, &provider, "VLabc");

        assert!(matches!(
            result,
            Err(LibraryError::Provider(PlaylistError::Rejected))
        ));
    }
}
