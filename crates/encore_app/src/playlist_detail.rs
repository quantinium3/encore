use std::path::Path;
use std::sync::Arc;

use anyhow::Context as _;
use encore_core::{
    LibraryError, PlaylistDetail, PlaylistPrivacy as CorePlaylistPrivacy, Track,
    fetch_playlist_detail,
};
use encore_innertube::InnertubeValidator;
use encore_storage::SqliteStore;
use encore_ui::{
    PlaylistDetailModel, PlaylistDetailSummary, PlaylistPrivacy, PlaylistsModel, TrackSummary,
};
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
    playlists: &Entity<PlaylistsModel>,
    detail: &Entity<PlaylistDetailModel>,
    window: &mut Window,
    cx: &mut App,
) {
    let Wiring { store, validator } = wiring;

    refresh(&store, &validator, playlists, detail, window, cx);

    window
        .observe(playlists, cx, {
            let store = store.clone();
            let validator = validator.clone();
            let detail = detail.clone();
            move |playlists, window, cx| {
                refresh(&store, &validator, &playlists, &detail, window, cx)
            }
        })
        .detach();
}

fn refresh(
    store: &Arc<SqliteStore>,
    validator: &Arc<InnertubeValidator>,
    playlists: &Entity<PlaylistsModel>,
    detail: &Entity<PlaylistDetailModel>,
    window: &mut Window,
    cx: &mut App,
) {
    let active = playlists.read(cx).active().cloned();
    let current = detail.read(cx).detail().map(|found| found.playlist_id.clone());

    match active {
        None if current.is_some() => {
            detail.update(cx, |model, cx| model.set_detail(None, cx));
        }
        None => {}
        Some(id) if Some(&id) == current.as_ref() => {}
        Some(id) => {
            let store = store.clone();
            let validator = validator.clone();
            let detail = detail.clone();
            let fetch_id = id.clone();
            window
                .spawn(cx, async move |cx| {
                    let result = run_blocking(move || {
                        fetch_playlist_detail(store.as_ref(), validator.as_ref(), &fetch_id)
                    })
                    .await;
                    cx.update(|_, cx| apply(result, &id, &detail, cx)).ok();
                })
                .detach();
        }
    }
}

fn apply(
    result: Option<Result<PlaylistDetail, LibraryError>>,
    playlist_id: &SharedString,
    detail: &Entity<PlaylistDetailModel>,
    cx: &mut App,
) {
    match result {
        Some(Ok(found)) => {
            let summary = to_summary(playlist_id.clone(), found);
            detail.update(cx, |model, cx| model.set_detail(Some(summary), cx));
        }
        Some(Err(LibraryError::SignedOut)) => {
            detail.update(cx, |model, cx| model.set_detail(None, cx));
        }
        Some(Err(error)) => eprintln!("encore: could not fetch the playlist: {error}"),
        None => eprintln!("encore: the background task stopped unexpectedly."),
    }
}

fn to_summary(playlist_id: SharedString, detail: PlaylistDetail) -> PlaylistDetailSummary {
    PlaylistDetailSummary {
        playlist_id,
        title: SharedString::from(detail.title),
        description: detail.description.map(SharedString::from),
        thumbnail_url: detail.thumbnail_url.map(SharedString::from),
        track_count: detail.track_count.map(|count| count as usize),
        year: detail.year.map(SharedString::from),
        total_duration: detail.total_duration.map(SharedString::from),
        privacy: detail.privacy.map(to_privacy),
        tracks: detail.tracks.into_iter().map(to_track).collect(),
    }
}

fn to_privacy(privacy: CorePlaylistPrivacy) -> PlaylistPrivacy {
    match privacy {
        CorePlaylistPrivacy::Public => PlaylistPrivacy::Public,
        CorePlaylistPrivacy::Private => PlaylistPrivacy::Private,
        CorePlaylistPrivacy::Unlisted => PlaylistPrivacy::Unlisted,
    }
}

fn to_track(track: Track) -> TrackSummary {
    TrackSummary {
        id: SharedString::from(track.id),
        title: SharedString::from(track.title),
        artist: track.artist.map(SharedString::from),
        artist_id: track.artist_id.map(SharedString::from),
        album: track.album.map(SharedString::from),
        duration: track.duration.map(SharedString::from),
        thumbnail_url: track.thumbnail_url.map(SharedString::from),
        credits: track.credits.into_iter().map(SharedString::from).collect(),
    }
}
