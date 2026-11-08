use std::path::Path;
use std::sync::Arc;

use anyhow::Context as _;
use encore_core::{Album, Artist as CoreArtist, ArtistPage, LibraryError, Track, fetch_artist};
use encore_innertube::InnertubeValidator;
use encore_storage::SqliteStore;
use encore_ui::{ArtistModel, ArtistSummary, CardSummary, TrackSummary};
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

pub fn attach(wiring: Wiring, artist: &Entity<ArtistModel>, window: &mut Window, cx: &mut App) {
    let Wiring { store, validator } = wiring;

    refresh(&store, &validator, artist, window, cx);

    window
        .observe(artist, cx, {
            let store = store.clone();
            let validator = validator.clone();
            move |artist, window, cx| refresh(&store, &validator, &artist, window, cx)
        })
        .detach();
}

fn refresh(
    store: &Arc<SqliteStore>,
    validator: &Arc<InnertubeValidator>,
    artist: &Entity<ArtistModel>,
    window: &mut Window,
    cx: &mut App,
) {
    let active = artist.read(cx).active().cloned();
    let current = artist.read(cx).page().map(|page| page.artist_id.clone());

    match active {
        Some(id) if Some(&id) == current.as_ref() => {}
        Some(id) => {
            let store = store.clone();
            let validator = validator.clone();
            let artist = artist.clone();
            let fetch_id = id.clone();
            window
                .spawn(cx, async move |cx| {
                    let result = run_blocking(move || {
                        fetch_artist(store.as_ref(), validator.as_ref(), &fetch_id)
                    })
                    .await;
                    cx.update(|_, cx| apply(result, &id, &artist, cx)).ok();
                })
                .detach();
        }
        None => {}
    }
}

fn apply(
    result: Option<Result<ArtistPage, LibraryError>>,
    artist_id: &SharedString,
    artist: &Entity<ArtistModel>,
    cx: &mut App,
) {
    match result {
        Some(Ok(page)) => {
            let summary = to_summary(artist_id.clone(), page);
            artist.update(cx, |model, cx| model.set_page(Some(summary), cx));
        }
        Some(Err(LibraryError::SignedOut)) => {
            artist.update(cx, |model, cx| model.set_page(None, cx));
        }
        Some(Err(error)) => {
            eprintln!("encore: could not fetch the artist: {error}");
            artist.update(cx, |model, cx| model.set_page(None, cx));
        }
        None => eprintln!("encore: the background task stopped unexpectedly."),
    }
}

fn to_summary(artist_id: SharedString, page: ArtistPage) -> ArtistSummary {
    ArtistSummary {
        artist_id,
        name: SharedString::from(page.name),
        description: page.description.map(SharedString::from),
        subscribers: page.subscribers.map(SharedString::from),
        header_image_url: page.header_image_url.map(SharedString::from),
        tracks: page.tracks.into_iter().map(to_track).collect(),
        tracks_playlist_id: page.tracks_playlist_id.map(SharedString::from),
        albums: page.albums.into_iter().map(to_album_card).collect(),
        singles: page.singles.into_iter().map(to_album_card).collect(),
        similar_artists: page
            .similar_artists
            .into_iter()
            .map(to_artist_card)
            .collect(),
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

fn to_album_card(album: Album) -> CardSummary {
    CardSummary {
        id: SharedString::from(album.id),
        title: SharedString::from(album.title),
        subtitle: album.year.map(SharedString::from),
        thumbnail_url: album.thumbnail_url.map(SharedString::from),
    }
}

fn to_artist_card(artist: CoreArtist) -> CardSummary {
    CardSummary {
        id: SharedString::from(artist.id),
        title: SharedString::from(artist.name),
        subtitle: artist.subscribers.map(SharedString::from),
        thumbnail_url: artist.thumbnail_url.map(SharedString::from),
    }
}
