use std::path::Path;
use std::sync::Arc;

use anyhow::Context as _;
use encore_core::{LibraryError, fetch_playlists};
use encore_innertube::InnertubeValidator;
use encore_storage::SqliteStore;
use encore_ui::{AccountsModel, PlaylistSummary, PlaylistsModel};
use gpui_kit::{App, Entity, SharedString, Window};

use crate::accounts::{DATABASE_FILE, run_blocking};

pub struct Wiring {
    store: Arc<SqliteStore>,
    validator: Arc<InnertubeValidator>,
}

pub fn build(data_dir: &Path) -> anyhow::Result<Wiring> {
    let store = Arc::new(
        SqliteStore::new(data_dir.join(DATABASE_FILE)).context("could not open the database")?,
    );
    Ok(Wiring {
        store,
        validator: Arc::new(InnertubeValidator::new()),
    })
}

pub fn attach(
    wiring: Wiring,
    accounts: &Entity<AccountsModel>,
    playlists: &Entity<PlaylistsModel>,
    window: &mut Window,
    cx: &mut App,
) {
    let Wiring { store, validator } = wiring;

    refresh(&store, &validator, accounts, playlists, window, cx);

    window
        .observe(accounts, cx, {
            let store = store.clone();
            let validator = validator.clone();
            let playlists = playlists.clone();
            move |accounts, window, cx| {
                refresh(&store, &validator, &accounts, &playlists, window, cx)
            }
        })
        .detach();
}

fn refresh(
    store: &Arc<SqliteStore>,
    validator: &Arc<InnertubeValidator>,
    accounts: &Entity<AccountsModel>,
    playlists: &Entity<PlaylistsModel>,
    window: &mut Window,
    cx: &mut App,
) {
    match accounts.read(cx).active() {
        None => playlists.update(cx, |model, cx| model.set_playlists(Vec::new(), cx)),
        Some(_) => {
            let store = store.clone();
            let validator = validator.clone();
            let playlists = playlists.clone();
            window
                .spawn(cx, async move |cx| {
                    let result =
                        run_blocking(move || fetch_playlists(store.as_ref(), validator.as_ref()))
                            .await;
                    cx.update(|_, cx| apply(result, &playlists, cx)).ok();
                })
                .detach();
        }
    }
}

fn apply(
    result: Option<Result<Vec<encore_core::Playlist>, LibraryError>>,
    playlists: &Entity<PlaylistsModel>,
    cx: &mut App,
) {
    match result {
        Some(Ok(fetched)) => {
            let summaries = fetched.into_iter().map(summary).collect();
            playlists.update(cx, |model, cx| model.set_playlists(summaries, cx));
        }
        Some(Err(LibraryError::SignedOut)) => {
            playlists.update(cx, |model, cx| model.set_playlists(Vec::new(), cx));
        }
        Some(Err(error)) => eprintln!("encore: could not fetch playlists: {error}"),
        None => eprintln!("encore: the background task stopped unexpectedly."),
    }
}

fn summary(playlist: encore_core::Playlist) -> PlaylistSummary {
    PlaylistSummary {
        id: SharedString::from(playlist.id),
        title: SharedString::from(playlist.title),
        track_count: playlist.track_count.map(|count| count as usize),
        thumbnail_url: playlist.thumbnail_url.map(SharedString::from),
        owner: playlist.owner.map(SharedString::from),
    }
}
