use std::path::Path;
use std::sync::Arc;

use anyhow::Context as _;
use encore_core::{
    Album, Artist, LibraryError, MixedItem, MixedResults, Playlist, SearchKind as CoreKind,
    SearchResults, Track, TopResult, search,
};
use encore_innertube::InnertubeValidator;
use encore_storage::SqliteStore;
use encore_ui::{
    CardSummary, MixedItemSummary, MixedResultsSummary, SearchEvent, SearchKind, SearchModel,
    SearchState, TopResultSummary, TrackSummary,
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

pub fn attach(wiring: Wiring, search_model: &Entity<SearchModel>, window: &mut Window, cx: &mut App) {
    let Wiring { store, validator } = wiring;

    window
        .subscribe(search_model, cx, {
            let search_model = search_model.clone();
            move |_, event: &SearchEvent, window, cx| {
                let SearchEvent::Requested { query, kind } = event;
                let store = store.clone();
                let validator = validator.clone();
                let search_model = search_model.clone();
                let query = query.clone();
                let kind = *kind;
                window
                    .spawn(cx, async move |cx| {
                        let text = query.to_string();
                        let core_kind = to_core_kind(kind);
                        let result = run_blocking(move || {
                            search(store.as_ref(), validator.as_ref(), core_kind, &text)
                        })
                        .await;
                        cx.update(|_, cx| apply(result, &query, kind, &search_model, cx))
                            .ok();
                    })
                    .detach();
            }
        })
        .detach();
}

fn apply(
    result: Option<Result<SearchResults, LibraryError>>,
    query: &SharedString,
    kind: SearchKind,
    search_model: &Entity<SearchModel>,
    cx: &mut App,
) {
    match result {
        Some(Ok(results)) => {
            let state = to_state(results);
            search_model.update(cx, |model, cx| model.set_results(query, kind, state, cx));
        }
        Some(Err(LibraryError::SignedOut)) => {
            search_model.update(cx, |model, cx| {
                model.set_failed(query, kind, "Sign in to search.", cx)
            });
        }
        Some(Err(error)) => {
            eprintln!("encore: search failed: {error}");
            search_model.update(cx, |model, cx| {
                model.set_failed(query, kind, "Search failed. Try again.", cx)
            });
        }
        None => eprintln!("encore: the background task stopped unexpectedly."),
    }
}

fn to_core_kind(kind: SearchKind) -> CoreKind {
    match kind {
        SearchKind::All => CoreKind::All,
        SearchKind::Songs => CoreKind::Songs,
        SearchKind::Videos => CoreKind::Videos,
        SearchKind::Albums => CoreKind::Albums,
        SearchKind::Artists => CoreKind::Artists,
        SearchKind::Playlists => CoreKind::Playlists,
    }
}

fn to_state(results: SearchResults) -> SearchState {
    match results {
        SearchResults::All(mixed) => SearchState::All(to_mixed_results(mixed)),
        SearchResults::Songs(tracks) => SearchState::Songs(tracks.into_iter().map(to_track).collect()),
        SearchResults::Videos(tracks) => SearchState::Videos(tracks.into_iter().map(to_track).collect()),
        SearchResults::Albums(albums) => SearchState::Albums(albums.into_iter().map(to_album_card).collect()),
        SearchResults::Artists(artists) => {
            SearchState::Artists(artists.into_iter().map(to_artist_card).collect())
        }
        SearchResults::Playlists(playlists) => {
            SearchState::Playlists(playlists.into_iter().map(to_playlist_card).collect())
        }
    }
}

fn to_mixed_results(mixed: MixedResults) -> MixedResultsSummary {
    MixedResultsSummary {
        top_result: mixed.top_result.map(to_top_result),
        items: mixed.items.into_iter().map(to_mixed_item).collect(),
    }
}

fn to_top_result(top_result: TopResult) -> TopResultSummary {
    TopResultSummary {
        item: to_mixed_item(top_result.item),
        quick_picks: top_result.quick_picks.into_iter().map(to_track).collect(),
    }
}

fn to_mixed_item(item: MixedItem) -> MixedItemSummary {
    match item {
        MixedItem::Track(track) => MixedItemSummary::Track(to_track(track)),
        MixedItem::Album(album) => MixedItemSummary::Album(to_album_card(album)),
        MixedItem::Artist(artist) => MixedItemSummary::Artist(to_artist_card(artist)),
        MixedItem::Playlist(playlist) => MixedItemSummary::Playlist(to_playlist_card(playlist)),
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
    let subtitle = join(&[album.artist, album.year]);
    CardSummary {
        id: SharedString::from(album.id),
        title: SharedString::from(album.title),
        subtitle: subtitle.map(SharedString::from),
        thumbnail_url: album.thumbnail_url.map(SharedString::from),
    }
}

fn to_artist_card(artist: Artist) -> CardSummary {
    CardSummary {
        id: SharedString::from(artist.id),
        title: SharedString::from(artist.name),
        subtitle: artist.subscribers.map(SharedString::from),
        thumbnail_url: artist.thumbnail_url.map(SharedString::from),
    }
}

fn to_playlist_card(playlist: Playlist) -> CardSummary {
    let subtitle = playlist.track_count.map(|count| format!("{count} songs"));
    CardSummary {
        id: SharedString::from(playlist.id),
        title: SharedString::from(playlist.title),
        subtitle: subtitle.map(SharedString::from),
        thumbnail_url: playlist.thumbnail_url.map(SharedString::from),
    }
}

fn join(parts: &[Option<String>]) -> Option<String> {
    let joined = parts
        .iter()
        .filter_map(|part| part.as_deref())
        .collect::<Vec<_>>()
        .join(" • ");
    (!joined.is_empty()).then_some(joined)
}
