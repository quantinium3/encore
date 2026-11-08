use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::Context as _;
use encore_core::{PlaybackQueue, RepeatMode, Track, fetch_radio};
use encore_innertube::InnertubeValidator;
use encore_player::{Player, PlayerError};
use encore_storage::SqliteStore;
use encore_stream::{ResolveError, StreamResolver, StreamSource};
use encore_ui::{PlayerEvent, PlayerModel, TrackSummary};
use futures::StreamExt as _;
use futures::channel::mpsc::{UnboundedReceiver, unbounded};
use gpui_kit::{App, Entity, SharedString, Window};
use thiserror::Error;

use crate::accounts::{DATABASE_FILE, run_blocking};

const MAX_RETRIES: u32 = 5;

const AUTOPLAY_LOW_WATER: usize = 3;

pub(crate) struct Lookahead {
    index: usize,
    video_id: String,
    source: StreamSource,
}

type LastAttempt = Arc<Mutex<Option<(String, StreamSource)>>>;
type RetryCount = Arc<Mutex<Option<(String, u32)>>>;

#[derive(Clone)]
pub(crate) struct Playback {
    pub(crate) player: Arc<Player>,
    pub(crate) resolver: Arc<StreamResolver>,
    pub(crate) queue: Arc<Mutex<PlaybackQueue>>,
    pub(crate) last_attempt: LastAttempt,
    pub(crate) retry_count: RetryCount,
    pub(crate) model: Entity<PlayerModel>,
    pub(crate) store: Arc<SqliteStore>,
    pub(crate) validator: Arc<InnertubeValidator>,
    pub(crate) fetching_radio: Arc<AtomicBool>,
    pub(crate) lookahead: Arc<Mutex<Option<Lookahead>>>,
    pub(crate) generation: Arc<AtomicU64>,
    pub(crate) priming: Arc<AtomicBool>,
}

pub struct Wiring {
    player: Arc<Player>,
    resolver: Arc<StreamResolver>,
    events: UnboundedReceiver<encore_player::PlayerEvent>,
    store: Arc<SqliteStore>,
    validator: Arc<InnertubeValidator>,
}

pub fn build(data_dir: &Path) -> anyhow::Result<Wiring> {
    let (sender, events) = unbounded();
    let player = Player::new(move |event| {
        let _ = sender.unbounded_send(event);
    })
    .map_err(|error| anyhow::anyhow!("could not start the player: {error}"))?;
    let resolver = StreamResolver::new()
        .map_err(|error| anyhow::anyhow!("could not start the stream resolver: {error}"))?;
    Ok(Wiring {
        player: Arc::new(player),
        resolver: Arc::new(resolver),
        events,
        store: Arc::new(
            SqliteStore::new(data_dir.join(DATABASE_FILE))
                .context("could not open the database")?,
        ),
        validator: Arc::new(InnertubeValidator::new()),
    })
}

pub fn attach(wiring: Wiring, model: &Entity<PlayerModel>, window: &mut Window, cx: &mut App) {
    let Wiring {
        player,
        resolver,
        events,
        store,
        validator,
    } = wiring;
    let playback = Playback {
        player,
        resolver,
        queue: Arc::new(Mutex::new(PlaybackQueue::new(Vec::new()))),
        last_attempt: Arc::new(Mutex::new(None)),
        retry_count: Arc::new(Mutex::new(None)),
        model: model.clone(),
        store,
        validator,
        fetching_radio: Arc::new(AtomicBool::new(false)),
        lookahead: Arc::new(Mutex::new(None)),
        generation: Arc::new(AtomicU64::new(0)),
        priming: Arc::new(AtomicBool::new(false)),
    };

    report(
        playback.player.set_volume(model.read(cx).volume_percent()),
        "set the initial volume",
    );

    window
        .subscribe(model, cx, {
            let playback = playback.clone();
            move |_, event: &PlayerEvent, window, cx| {
                handle_request(&playback, event, window, cx);
            }
        })
        .detach();

    listen_for_player_events(playback, events, window, cx);
}

fn handle_request(playback: &Playback, event: &PlayerEvent, window: &mut Window, cx: &mut App) {
    match event {
        PlayerEvent::PlayQueueRequested {
            tracks,
            start_index,
        } => {
            let tracks: Vec<Track> = tracks.iter().map(to_core_track).collect();
            let mut playing = PlaybackQueue::new(tracks);
            playing.play_at(*start_index);
            *lock(&playback.queue) = playing;
            play_current(playback, window, cx);
        }
        PlayerEvent::PlayPauseRequested => {
            report(playback.player.toggle_play_pause(), "toggle playback");
        }
        PlayerEvent::NextRequested => {
            lock(&playback.queue).skip_forward();
            play_current(playback, window, cx);
        }
        PlayerEvent::PreviousRequested => {
            lock(&playback.queue).skip_backward();
            play_current(playback, window, cx);
        }
        PlayerEvent::ShuffleToggleRequested => {
            let shuffle = {
                let mut playing = lock(&playback.queue);
                let shuffle = !playing.shuffle();
                playing.set_shuffle(shuffle);
                shuffle
            };
            reset_lookahead(playback, window, cx);
            playback
                .model
                .update(cx, |model, cx| model.set_shuffle(shuffle, cx));
        }
        PlayerEvent::RepeatToggleRequested => {
            let repeat_on = {
                let mut playing = lock(&playback.queue);
                let repeat_on = playing.repeat() == RepeatMode::Off;
                let mode = match repeat_on {
                    true => RepeatMode::All,
                    false => RepeatMode::Off,
                };
                playing.set_repeat(mode);
                repeat_on
            };
            reset_lookahead(playback, window, cx);
            playback
                .model
                .update(cx, |model, cx| model.set_repeat(repeat_on, cx));
        }
        PlayerEvent::SeekRequested(position) => {
            report(playback.player.seek(*position), "seek");
        }
        PlayerEvent::VolumeRequested(percent) => {
            report(playback.player.set_volume(*percent), "set the volume");
            playback
                .model
                .update(cx, |model, cx| model.set_volume_percent(*percent, cx));
        }
    }
}

pub(crate) fn play_current(playback: &Playback, window: &mut Window, cx: &mut App) {
    playback.generation.fetch_add(1, Ordering::SeqCst);
    *lock(&playback.lookahead) = None;

    let video_id = lock(&playback.queue)
        .current_track()
        .map(|track| track.id.clone());
    publish_queue(playback, cx);

    top_up_radio(playback, window, cx);

    let Some(video_id) = video_id else { return };
    let player = playback.player.clone();
    let resolver = playback.resolver.clone();
    let last_attempt = playback.last_attempt.clone();
    let playback = playback.clone();
    window
        .spawn(cx, async move |cx| {
            let video_id_for_load = video_id.clone();
            let result = run_blocking(move || {
                load(&resolver, &player, &video_id_for_load).map_err(|error| error.to_string())
            })
            .await;
            match result {
                Some(Ok(source)) => {
                    *lock(&last_attempt) = Some((video_id, source));
                    cx.update(|window, cx| prime_lookahead(&playback, window, cx))
                        .ok();
                }
                Some(Err(error)) => eprintln!("encore: could not play this track: {error}"),
                None => eprintln!("encore: the background task stopped unexpectedly."),
            }
        })
        .detach();
}

pub(crate) fn publish_queue(playback: &Playback, cx: &mut App) {
    let (summaries, current_index) = {
        let playing = lock(&playback.queue);
        let summaries: Vec<TrackSummary> = playing.tracks().iter().map(to_ui_track).collect();
        (summaries, playing.current_index())
    };
    playback
        .model
        .update(cx, |model, cx| model.set_queue(summaries, current_index, cx));
}

fn reset_lookahead(playback: &Playback, window: &mut Window, cx: &mut App) {
    playback.generation.fetch_add(1, Ordering::SeqCst);
    *lock(&playback.lookahead) = None;
    report(playback.player.clear_lookahead(), "clear the next track");
    prime_lookahead(playback, window, cx);
}

fn prime_lookahead(playback: &Playback, window: &mut Window, cx: &mut App) {
    let (index, video_id) = {
        let playing = lock(&playback.queue);
        match playing.peek_next() {
            Some(index) => (index, playing.tracks()[index].id.clone()),
            None => return,
        }
    };
    if lock(&playback.lookahead)
        .as_ref()
        .is_some_and(|primed| primed.index == index && primed.video_id == video_id)
    {
        return;
    }
    if playback.priming.swap(true, Ordering::SeqCst) {
        return;
    }

    let generation = playback.generation.load(Ordering::SeqCst);
    let resolver = playback.resolver.clone();
    let playback = playback.clone();
    window
        .spawn(cx, async move |cx| {
            let id = video_id.clone();
            let result =
                run_blocking(move || resolver.resolve(&id).map_err(|error| error.to_string()))
                    .await;
            cx.update(|window, cx| {
                playback.priming.store(false, Ordering::SeqCst);
                if playback.generation.load(Ordering::SeqCst) != generation {
                    prime_lookahead(&playback, window, cx);
                    return;
                }
                let stream = match result {
                    Some(Ok(stream)) => stream,
                    Some(Err(error)) => {
                        eprintln!("encore: could not line up the next track: {error}");
                        return;
                    }
                    None => return,
                };
                let still_next = {
                    let playing = lock(&playback.queue);
                    playing.peek_next() == Some(index)
                        && playing.tracks().get(index).is_some_and(|t| t.id == video_id)
                };
                if !still_next {
                    prime_lookahead(&playback, window, cx);
                    return;
                }
                match playback.player.enqueue(&stream.url, &stream.headers) {
                    Ok(()) => {
                        *lock(&playback.lookahead) = Some(Lookahead {
                            index,
                            video_id,
                            source: stream.source,
                        });
                    }
                    Err(error) => eprintln!("encore: could not line up the next track: {error}"),
                }
            })
            .ok();
        })
        .detach();
}

fn load(
    resolver: &StreamResolver,
    player: &Player,
    video_id: &str,
) -> Result<StreamSource, PlaybackError> {
    let stream = resolver.resolve(video_id).map_err(PlaybackError::Resolve)?;
    player
        .load(&stream.url, &stream.headers)
        .map_err(PlaybackError::Player)?;
    Ok(stream.source)
}

fn skip_failed(playback: &Playback, window: &mut Window, cx: &mut App) {
    lock(&playback.queue).advance();
    play_current(playback, window, cx);
}

fn on_track_failed(playback: &Playback, error: &str, window: &mut Window, cx: &mut App) {
    let Some(video_id) = lock(&playback.queue)
        .current_track()
        .map(|track| track.id.clone())
    else {
        eprintln!("encore: mpv could not play <unknown track> ({error}); skipping it");
        skip_failed(playback, window, cx);
        return;
    };

    let retries = {
        let mut retry_count = lock(&playback.retry_count);
        let retries = match retry_count.as_ref() {
            Some((id, count)) if *id == video_id => count + 1,
            _ => 1,
        };
        *retry_count = Some((video_id.clone(), retries));
        retries
    };

    if retries > MAX_RETRIES {
        eprintln!(
            "encore: mpv still could not play {video_id} after {MAX_RETRIES} retries ({error}); skipping it"
        );
        skip_failed(playback, window, cx);
        return;
    }

    let attempt = lock(&playback.last_attempt).clone();
    match attempt {
        Some((attempted, StreamSource::Rustypipe)) if attempted == video_id && retries == 1 => {
            eprintln!(
                "encore: mpv could not play {video_id} via rustypipe ({error}); retrying via yt-dlp (attempt {retries}/{MAX_RETRIES})"
            );
            retry_via_yt_dlp(playback, video_id, window, cx);
        }
        _ => {
            eprintln!(
                "encore: mpv could not play {video_id} ({error}); retrying (attempt {retries}/{MAX_RETRIES})"
            );
            retry_track(playback, video_id, window, cx);
        }
    }
}

fn on_track_ended(playback: &Playback, window: &mut Window, cx: &mut App) {
    let primed = lock(&playback.lookahead).take();
    let next = lock(&playback.queue).peek_next();
    match primed {
        Some(primed) if next == Some(primed.index) && !playback.player.is_idle() => {
            lock(&playback.queue).advance();
            *lock(&playback.last_attempt) = Some((primed.video_id, primed.source));
            publish_queue(playback, cx);
            top_up_radio(playback, window, cx);
            prime_lookahead(playback, window, cx);
        }
        _ => {
            lock(&playback.queue).advance();
            play_current(playback, window, cx);
        }
    }
}

fn top_up_radio(playback: &Playback, window: &mut Window, cx: &mut App) {
    let seed = {
        let playing = lock(&playback.queue);
        if playing.current_index().is_none() || playing.remaining() > AUTOPLAY_LOW_WATER {
            return;
        }
        match playing.tracks().last() {
            Some(last) => last.id.clone(),
            None => return,
        }
    };
    if playback.fetching_radio.swap(true, Ordering::SeqCst) {
        return;
    }

    let store = playback.store.clone();
    let validator = playback.validator.clone();
    let queue = playback.queue.clone();
    let model = playback.model.clone();
    let fetching = playback.fetching_radio.clone();
    let playback = playback.clone();
    window
        .spawn(cx, async move |cx| {
            let result = run_blocking(move || {
                fetch_radio(store.as_ref(), validator.as_ref(), &seed).map_err(|e| e.to_string())
            })
            .await;
            match result {
                Some(Ok(tracks)) => {
                    let added = lock(&queue).append(tracks);
                    let summaries: Vec<TrackSummary> = added.iter().map(to_ui_track).collect();
                    cx.update(|window, cx| {
                        model.update(cx, |model, cx| model.append_queue(summaries, cx));
                        prime_lookahead(&playback, window, cx);
                    })
                    .ok();
                }
                Some(Err(error)) => eprintln!("encore: could not load more tracks: {error}"),
                None => eprintln!("encore: the background task stopped unexpectedly."),
            }
            fetching.store(false, Ordering::SeqCst);
        })
        .detach();
}

fn retry_via_yt_dlp(playback: &Playback, video_id: String, window: &mut Window, cx: &mut App) {
    *lock(&playback.last_attempt) = Some((video_id.clone(), StreamSource::YtDlp));
    let player = playback.player.clone();
    let resolver = playback.resolver.clone();
    window
        .spawn(cx, async move |_| {
            let result = run_blocking(move || {
                let stream = resolver
                    .resolve_via_yt_dlp(&video_id)
                    .map_err(|error| error.to_string())?;
                player
                    .load(&stream.url, &stream.headers)
                    .map_err(|error| error.to_string())
            })
            .await;
            match result {
                Some(Ok(())) => {}
                Some(Err(error)) => eprintln!("encore: the yt-dlp fallback also failed: {error}"),
                None => eprintln!("encore: the background task stopped unexpectedly."),
            }
        })
        .detach();
}

fn retry_track(playback: &Playback, video_id: String, window: &mut Window, cx: &mut App) {
    let player = playback.player.clone();
    let resolver = playback.resolver.clone();
    let last_attempt = playback.last_attempt.clone();
    window
        .spawn(cx, async move |_| {
            let video_id_for_load = video_id.clone();
            let result = run_blocking(move || {
                load(&resolver, &player, &video_id_for_load).map_err(|error| error.to_string())
            })
            .await;
            match result {
                Some(Ok(source)) => {
                    *lock(&last_attempt) = Some((video_id, source));
                }
                Some(Err(error)) => {
                    eprintln!("encore: retrying {video_id} failed: {error}");
                }
                None => eprintln!("encore: the background task stopped unexpectedly."),
            }
        })
        .detach();
}

fn listen_for_player_events(
    playback: Playback,
    mut events: UnboundedReceiver<encore_player::PlayerEvent>,
    window: &mut Window,
    cx: &mut App,
) {
    window
        .spawn(cx, async move |cx| {
            while let Some(event) = events.next().await {
                let playback = playback.clone();
                cx.update(|window, cx| match event {
                    encore_player::PlayerEvent::Position(position) => {
                        playback
                            .model
                            .update(cx, |model, cx| model.set_position_secs(position, cx));
                    }
                    encore_player::PlayerEvent::Duration(duration) => {
                        playback
                            .model
                            .update(cx, |model, cx| model.set_duration_secs(duration, cx));
                    }
                    encore_player::PlayerEvent::Playing(playing) => {
                        playback
                            .model
                            .update(cx, |model, cx| model.set_playing(playing, cx));
                    }
                    encore_player::PlayerEvent::TrackEnded => {
                        on_track_ended(&playback, window, cx);
                    }
                    encore_player::PlayerEvent::TrackFailed(error) => {
                        on_track_failed(&playback, &error, window, cx);
                    }
                })
                .ok();
            }
        })
        .detach();
}

#[derive(Debug, Error)]
enum PlaybackError {
    #[error(transparent)]
    Resolve(#[from] ResolveError),
    #[error(transparent)]
    Player(#[from] PlayerError),
}

pub(crate) fn lock<T>(value: &Mutex<T>) -> MutexGuard<'_, T> {
    value.lock().unwrap_or_else(|e| e.into_inner())
}

pub(crate) fn report<T>(result: Result<T, PlayerError>, action: &str) {
    if let Err(error) = result {
        eprintln!("encore: could not {action}: {error}");
    }
}

pub(crate) fn to_core_track(track: &TrackSummary) -> Track {
    Track {
        id: track.id.to_string(),
        title: track.title.to_string(),
        artist: track.artist.as_ref().map(SharedString::to_string),
        artist_id: track.artist_id.as_ref().map(SharedString::to_string),
        album: track.album.as_ref().map(SharedString::to_string),
        duration: track.duration.as_ref().map(SharedString::to_string),
        thumbnail_url: track.thumbnail_url.as_ref().map(SharedString::to_string),
        credits: track.credits.iter().map(SharedString::to_string).collect(),
    }
}

pub(crate) fn to_ui_track(track: &Track) -> TrackSummary {
    TrackSummary {
        id: SharedString::from(track.id.clone()),
        title: SharedString::from(track.title.clone()),
        artist: track.artist.clone().map(SharedString::from),
        artist_id: track.artist_id.clone().map(SharedString::from),
        album: track.album.clone().map(SharedString::from),
        duration: track.duration.clone().map(SharedString::from),
        thumbnail_url: track.thumbnail_url.clone().map(SharedString::from),
        credits: track.credits.iter().cloned().map(SharedString::from).collect(),
    }
}
