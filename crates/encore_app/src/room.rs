use std::sync::Arc;
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};

use encore_core::{PlaybackQueue, RepeatMode, SettingKey, SettingsStore as _, Track};
use encore_storage::SqliteStore;
use encore_room::{Intent, Room, RoomState, RoomTrack, Snapshot, Update, generate_code};
use encore_ui::{PlayerEvent, RoomEvent, RoomModel};
use futures::StreamExt as _;
use futures::channel::mpsc::unbounded;
use gpui_kit::{App, Entity, Window};

use crate::accounts::run_blocking;
use crate::playback::{Playback, lock, play_current, report, to_ui_track};

const ROOM_URL: SettingKey = SettingKey::new("room", "server_url");
const DEFAULT_ROOM_URL: &str = "https://encore-rooms.quantinium.workers.dev";
const DRIFT_TOLERANCE_SECS: f64 = 1.5;
const MIN_SEEK_SECS: f64 = 1.;
const RESYNC_INTERVAL: Duration = Duration::from_secs(3);

#[derive(Default)]
pub struct RoomSession {
    room: Option<Arc<Room>>,
    epoch: u64,
    applied_rev: Option<u64>,
    target: Option<(RoomState, f64)>,
    last_resync: Option<Instant>,
}

pub fn default_url() -> String {
    std::env::var("ENCORE_ROOM_URL")
        .ok()
        .filter(|url| !url.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_ROOM_URL.to_owned())
}

fn room_url(store: &SqliteStore) -> String {
    match store.setting(ROOM_URL) {
        Ok(Some(url)) if !url.trim().is_empty() => url,
        _ => default_url(),
    }
}

pub fn active(playback: &Playback) -> bool {
    lock(&playback.room).room.is_some()
}

pub fn attach(playback: &Playback, model: &Entity<RoomModel>, window: &mut Window, cx: &mut App) {
    window
        .subscribe(model, cx, {
            let playback = playback.clone();
            let model = model.clone();
            move |_, event: &RoomEvent, window, cx| match event {
                RoomEvent::CreateRequested => join(&playback, &model, generate_code(), window, cx),
                RoomEvent::JoinRequested(code) => {
                    join(&playback, &model, code.to_string(), window, cx)
                }
                RoomEvent::LeaveRequested => leave(&playback, &model, None, cx),
            }
        })
        .detach();
}

fn join(
    playback: &Playback,
    model: &Entity<RoomModel>,
    code: String,
    window: &mut Window,
    cx: &mut App,
) {
    model.update(cx, |model, cx| model.set_joining(cx));
    let store = playback.store.clone();
    let playback = playback.clone();
    let model = model.clone();
    window
        .spawn(cx, async move |cx| {
            let result =
                run_blocking(move || {
                    let url = room_url(store.as_ref());
                    Room::join(&url, &code).map_err(|error| format!("{error:#}"))
                })
                .await;
            cx.update(|window, cx| match result {
                Some(Ok((room, updates))) => connected(&playback, &model, room, updates, window, cx),
                Some(Err(message)) => model.update(cx, |model, cx| {
                    model.set_idle(Some(message.into()), cx);
                }),
                None => model.update(cx, |model, cx| {
                    model.set_idle(Some("the background task stopped unexpectedly".into()), cx);
                }),
            })
            .ok();
        })
        .detach();
}

fn connected(
    playback: &Playback,
    model: &Entity<RoomModel>,
    room: Room,
    updates: std::sync::mpsc::Receiver<Update>,
    window: &mut Window,
    cx: &mut App,
) {
    let code = room.code().to_owned();
    let epoch = {
        let mut session = lock(&playback.room);
        session.epoch += 1;
        session.room = Some(Arc::new(room));
        session.applied_rev = None;
        session.target = None;
        session.epoch
    };
    lock(&playback.queue).set_shuffle(false);
    playback.model.update(cx, |player, cx| player.set_shuffle(false, cx));
    model.update(cx, |model, cx| model.set_connected(code.into(), 1, cx));

    let (sender, mut incoming) = unbounded();
    std::thread::spawn(move || {
        while let Ok(update) = updates.recv() {
            if sender.unbounded_send(update).is_err() {
                break;
            }
        }
    });

    let playback = playback.clone();
    let model = model.clone();
    window
        .spawn(cx, async move |cx| {
            while let Some(update) = incoming.next().await {
                let playback = playback.clone();
                let model = model.clone();
                let keep_going = cx
                    .update(|window, cx| {
                        if lock(&playback.room).epoch != epoch {
                            return false;
                        }
                        match update {
                            Update::Snapshot {
                                snapshot,
                                clock_offset_ms,
                            } => {
                                handle_snapshot(
                                    &playback,
                                    &model,
                                    snapshot,
                                    clock_offset_ms,
                                    window,
                                    cx,
                                );
                                true
                            }
                            Update::Lost(message) => {
                                leave(
                                    &playback,
                                    &model,
                                    Some(format!("Lost the room: {message}")),
                                    cx,
                                );
                                false
                            }
                        }
                    })
                    .unwrap_or(false);
                if !keep_going {
                    break;
                }
            }
        })
        .detach();
}

fn leave(playback: &Playback, model: &Entity<RoomModel>, error: Option<String>, cx: &mut App) {
    let room = {
        let mut session = lock(&playback.room);
        session.epoch += 1;
        session.target = None;
        session.applied_rev = None;
        session.room.take()
    };
    if let Some(room) = room {
        room.leave();
    }
    model.update(cx, |model, cx| model.set_idle(error.map(Into::into), cx));
}

fn handle_snapshot(
    playback: &Playback,
    model: &Entity<RoomModel>,
    snapshot: Snapshot,
    offset_ms: f64,
    window: &mut Window,
    cx: &mut App,
) {
    let code = lock(&playback.room)
        .room
        .as_ref()
        .map(|room| room.code().to_owned());
    let Some(code) = code else { return };
    model.update(cx, |model, cx| {
        model.set_connected(code.into(), snapshot.members, cx)
    });

    let fresh = {
        let mut session = lock(&playback.room);
        let fresh = session.applied_rev != Some(snapshot.state.rev);
        session.applied_rev = Some(snapshot.state.rev);
        if fresh {
            session.target = Some((snapshot.state.clone(), offset_ms));
        }
        fresh
    };
    if fresh {
        apply(playback, &snapshot.state, offset_ms, window, cx);
    }
}

fn apply(playback: &Playback, state: &RoomState, offset_ms: f64, window: &mut Window, cx: &mut App) {
    if state.tracks.is_empty() {
        return;
    }
    let incoming: Vec<Track> = state.tracks.iter().map(from_room_track).collect();
    let repeat_mode = match state.repeat {
        true => RepeatMode::All,
        false => RepeatMode::Off,
    };

    let (local_len, same_tracks, same_current) = {
        let queue = lock(&playback.queue);
        let local = queue.tracks();
        let same_tracks = local.len() == incoming.len()
            && local.iter().zip(&incoming).all(|(a, b)| a.id == b.id);
        let same_current = queue.current_index() == state.index
            && queue.current_track().map(|t| &t.id) == state.index.and_then(|i| incoming.get(i)).map(|t| &t.id);
        (local.len(), same_tracks, same_current)
    };

    let appended_only = !same_tracks && same_current && local_len < incoming.len() && {
        let queue = lock(&playback.queue);
        queue
            .tracks()
            .iter()
            .zip(&incoming)
            .all(|(a, b)| a.id == b.id)
    };

    if !same_tracks {
        let mut queue = PlaybackQueue::new(incoming.clone());
        if let Some(index) = state.index {
            queue.play_at(index);
        }
        queue.set_repeat(repeat_mode);
        *lock(&playback.queue) = queue;
        if appended_only {
            let extra: Vec<_> = incoming[local_len..].iter().map(to_ui_track).collect();
            playback
                .model
                .update(cx, |player, cx| player.append_queue(extra, cx));
        }
    } else {
        lock(&playback.queue).set_repeat(repeat_mode);
    }
    playback
        .model
        .update(cx, |player, cx| player.set_repeat(state.repeat, cx));

    let Some(index) = state.index else { return };
    if index >= incoming.len() {
        return;
    }

    let expected = state.position_at(now_ms() + offset_ms);
    let needs_load = !(same_current && (same_tracks || appended_only)) || playback.player.is_idle();
    if needs_load {
        lock(&playback.queue).play_at(index);
        play_current(playback, window, cx);
        if expected >= MIN_SEEK_SECS {
            *lock(&playback.pending_seek) = Some(expected);
        }
        return;
    }

    let current = playback.model.read(cx).position_secs();
    if (current - expected).abs() > DRIFT_TOLERANCE_SECS {
        report(playback.player.seek(expected), "sync the position");
    }
    enforce(playback, cx);
}

pub fn resync(playback: &Playback, cx: &mut App, force: bool) {
    let expected = {
        let mut session = lock(&playback.room);
        if !force
            && session
                .last_resync
                .is_some_and(|last| last.elapsed() < RESYNC_INTERVAL)
        {
            return;
        }
        let Some((state, offset_ms)) = session.target.as_ref() else {
            return;
        };
        if !state.playing {
            return;
        }
        let expected = state.position_at(now_ms() + offset_ms);
        session.last_resync = Some(Instant::now());
        expected
    };
    let model = playback.model.read(cx);
    if !model.status().is_playing() {
        return;
    }
    if (model.position_secs() - expected).abs() > DRIFT_TOLERANCE_SECS {
        report(playback.player.seek(expected), "catch up with the room");
    }
}

pub fn enforce(playback: &Playback, cx: &mut App) {
    let wants_playing = match &lock(&playback.room).target {
        Some((state, _)) => state.playing,
        None => return,
    };
    let status = playback.model.read(cx).status();
    match (status, wants_playing) {
        (encore_ui::PlaybackStatus::Playing, false) => {
            report(playback.player.pause(), "pause with the room")
        }
        (encore_ui::PlaybackStatus::Paused, true) => {
            report(playback.player.play(), "play with the room")
        }
        _ => {}
    }
}

pub fn route(playback: &Playback, event: &PlayerEvent, cx: &mut App) -> bool {
    if !active(playback) {
        return false;
    }
    let model = playback.model.read(cx);
    let intent = match event {
        PlayerEvent::PlayQueueRequested {
            tracks,
            start_index,
        } => Some(Intent::PlayQueue {
            tracks: tracks.iter().map(|t| to_room_track(&crate::playback::to_core_track(t))).collect(),
            start: *start_index,
        }),
        PlayerEvent::PlayPauseRequested => Some(match room_is_empty(playback) {
            true => match local_queue_intent(playback) {
                Some(intent) => intent,
                None => return true,
            },
            false => match model.status().is_playing() {
                true => Intent::Pause,
                false => Intent::Play,
            },
        }),
        PlayerEvent::NextRequested => Some(Intent::Next {
            expected_index: None,
        }),
        PlayerEvent::PreviousRequested => Some(Intent::Previous),
        PlayerEvent::RepeatToggleRequested => Some(Intent::SetRepeat {
            repeat: !model.repeat(),
        }),
        PlayerEvent::SeekRequested(position) => Some(Intent::Seek {
            position_secs: *position,
        }),
        PlayerEvent::ShuffleToggleRequested => return true,
        PlayerEvent::VolumeRequested(_) => return false,
    };
    if let Some(intent) = intent {
        send(playback, intent);
    }
    true
}

fn room_is_empty(playback: &Playback) -> bool {
    lock(&playback.room)
        .target
        .as_ref()
        .is_none_or(|(state, _)| state.index.is_none())
}

fn local_queue_intent(playback: &Playback) -> Option<Intent> {
    let queue = lock(&playback.queue);
    if queue.tracks().is_empty() {
        return None;
    }
    Some(Intent::PlayQueue {
        tracks: queue.tracks().iter().map(to_room_track).collect(),
        start: queue.current_index().unwrap_or(0),
    })
}

pub fn send(playback: &Playback, intent: Intent) {
    let room = lock(&playback.room).room.clone();
    if let Some(room) = room {
        room.send(intent);
    }
}

pub fn track_ended(playback: &Playback, cx: &mut App) {
    if playback.model.read(cx).duration_secs() <= 0. {
        return;
    }
    skip_current(playback);
}

pub fn skip_current(playback: &Playback) {
    let index = lock(&playback.queue).current_index();
    send(playback, Intent::Next { expected_index: index });
}

pub fn append_radio(playback: &Playback, tracks: Vec<Track>) {
    let expected_len = lock(&playback.queue).tracks().len();
    send(
        playback,
        Intent::Append {
            tracks: tracks.iter().map(to_room_track).collect(),
            expected_len,
        },
    );
}

fn to_room_track(track: &Track) -> RoomTrack {
    RoomTrack {
        id: track.id.clone(),
        title: track.title.clone(),
        artist: track.artist.clone(),
        artist_id: track.artist_id.clone(),
        album: track.album.clone(),
        duration: track.duration.clone(),
        thumbnail_url: track.thumbnail_url.clone(),
        credits: track.credits.clone(),
    }
}

fn from_room_track(track: &RoomTrack) -> Track {
    Track {
        id: track.id.clone(),
        title: track.title.clone(),
        artist: track.artist.clone(),
        artist_id: track.artist_id.clone(),
        album: track.album.clone(),
        duration: track.duration.clone(),
        thumbnail_url: track.thumbnail_url.clone(),
        credits: track.credits.clone(),
    }
}

fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0., |elapsed| elapsed.as_secs_f64() * 1000.)
}
