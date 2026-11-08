use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::protocol::{RoomState, RoomTrack, Snapshot};

const MEMBER_TTL: Duration = Duration::from_secs(45);
const RESTART_THRESHOLD_SECS: f64 = 3.;
const MAX_TRACKS: usize = 500;

struct Data {
    state: RoomState,
    members: HashMap<String, Instant>,
}

struct RoomSlot {
    data: Mutex<Data>,
    changed: Condvar,
}

type Rooms = Arc<Mutex<HashMap<String, Arc<RoomSlot>>>>;

pub fn serve(listener: TcpListener, poll: Duration) {
    let rooms: Rooms = Arc::default();
    for stream in listener.incoming().flatten() {
        let rooms = rooms.clone();
        std::thread::spawn(move || {
            let _ = handle(stream, &rooms, poll);
        });
    }
}

fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0., |elapsed| elapsed.as_secs_f64() * 1000.)
}

fn handle(mut stream: TcpStream, rooms: &Rooms, poll: Duration) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_owned();
    let target = parts.next().unwrap_or("").to_owned();

    let mut length = 0usize;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let line = line.trim();
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;

    let (status, reply) = route(&method, &target, &body, rooms, poll);
    let text = reply.to_string();
    write!(
        stream,
        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{text}",
        text.len()
    )
}

fn query(query: &str, key: &str) -> Option<String> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(name, _)| *name == key)
        .map(|(_, value)| value.to_owned())
}

fn route(
    method: &str,
    target: &str,
    body: &[u8],
    rooms: &Rooms,
    poll: Duration,
) -> (&'static str, Value) {
    let (path, q) = target.split_once('?').unwrap_or((target, ""));
    let segments: Vec<&str> = path.trim_matches('/').split('/').collect();
    let ["room", code, action] = segments[..] else {
        return ("404 Not Found", json!({ "error": "not found" }));
    };
    let Some(client) = query(q, "client").filter(|c| !c.is_empty()) else {
        return ("400 Bad Request", json!({ "error": "missing client id" }));
    };
    let slot = rooms
        .lock()
        .unwrap()
        .entry(code.to_uppercase())
        .or_insert_with(|| {
            Arc::new(RoomSlot {
                data: Mutex::new(Data {
                    state: RoomState {
                        rev: 0,
                        tracks: Vec::new(),
                        index: None,
                        playing: false,
                        position_secs: 0.,
                        updated_at_ms: now_ms(),
                        repeat: false,
                    },
                    members: HashMap::new(),
                }),
                changed: Condvar::new(),
            })
        })
        .clone();

    let mut data = slot.data.lock().unwrap();
    if data.members.insert(client.clone(), Instant::now()).is_none() {
        slot.changed.notify_all();
    }
    match (method, action) {
        ("POST", "join") => ("200 OK", snapshot(&mut data)),
        ("POST", "leave") => {
            data.members.remove(&client);
            slot.changed.notify_all();
            ("200 OK", json!({ "ok": true }))
        }
        ("GET", "state") => {
            let since: u64 = query(q, "since").and_then(|s| s.parse().ok()).unwrap_or(0);
            if data.state.rev <= since {
                let (guard, _) = slot.changed.wait_timeout(data, poll).unwrap();
                data = guard;
            }
            ("200 OK", snapshot(&mut data))
        }
        ("POST", "intent") => match serde_json::from_slice::<Value>(body) {
            Ok(intent) => {
                if apply(&mut data.state, &intent) {
                    data.state.rev += 1;
                    slot.changed.notify_all();
                }
                ("200 OK", snapshot(&mut data))
            }
            Err(_) => ("400 Bad Request", json!({ "error": "bad json" })),
        },
        _ => ("405 Method Not Allowed", json!({ "error": "method" })),
    }
}

fn snapshot(data: &mut Data) -> Value {
    data.members.retain(|_, seen| seen.elapsed() < MEMBER_TTL);
    serde_json::to_value(Snapshot {
        state: data.state.clone(),
        members: data.members.len(),
        server_now_ms: now_ms(),
    })
    .unwrap_or(Value::Null)
}

fn tracks_of(value: &Value) -> Vec<RoomTrack> {
    serde_json::from_value::<Vec<RoomTrack>>(value.clone())
        .unwrap_or_default()
        .into_iter()
        .take(MAX_TRACKS)
        .collect()
}

fn apply(state: &mut RoomState, intent: &Value) -> bool {
    let now = now_ms();
    let position = |s: &RoomState| s.position_at(now);
    match intent["type"].as_str().unwrap_or("") {
        "play_queue" => {
            let tracks = tracks_of(&intent["tracks"]);
            let start = intent["start"].as_u64().unwrap_or(u64::MAX) as usize;
            if start >= tracks.len() {
                return false;
            }
            *state = RoomState {
                tracks,
                index: Some(start),
                playing: true,
                position_secs: 0.,
                updated_at_ms: now,
                ..state.clone()
            };
            true
        }
        "play" if state.index.is_some() && !state.playing => {
            state.playing = true;
            state.updated_at_ms = now;
            true
        }
        "pause" if state.index.is_some() && state.playing => {
            state.position_secs = position(state);
            state.playing = false;
            state.updated_at_ms = now;
            true
        }
        "seek" if state.index.is_some() => {
            let Some(target) = intent["position_secs"].as_f64() else {
                return false;
            };
            state.position_secs = target.max(0.);
            state.updated_at_ms = now;
            true
        }
        "next" => {
            let Some(index) = state.index else {
                return false;
            };
            if let Some(expected) = intent["expected_index"].as_u64() {
                if expected as usize != index {
                    return false;
                }
            }
            if index + 1 < state.tracks.len() {
                state.index = Some(index + 1);
                state.playing = true;
            } else if state.repeat {
                state.index = Some(0);
                state.playing = true;
            } else {
                state.playing = false;
            }
            state.position_secs = 0.;
            state.updated_at_ms = now;
            true
        }
        "previous" => {
            let Some(index) = state.index else {
                return false;
            };
            if position(state) <= RESTART_THRESHOLD_SECS && index > 0 {
                state.index = Some(index - 1);
            }
            state.position_secs = 0.;
            state.playing = true;
            state.updated_at_ms = now;
            true
        }
        "append" => {
            if intent["expected_len"].as_u64() != Some(state.tracks.len() as u64) {
                return false;
            }
            let mut added = Vec::new();
            for track in tracks_of(&intent["tracks"]) {
                let known = state.tracks.iter().chain(&added).any(|t| t.id == track.id);
                if !known && state.tracks.len() + added.len() < MAX_TRACKS {
                    added.push(track);
                }
            }
            if added.is_empty() {
                return false;
            }
            state.tracks.extend(added);
            true
        }
        "set_repeat" => {
            let repeat = intent["repeat"].as_bool().unwrap_or(false);
            if state.repeat == repeat {
                return false;
            }
            state.repeat = repeat;
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::Receiver;

    use super::*;
    use crate::{Intent, Room, Update};

    fn start() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || serve(listener, Duration::from_secs(3)));
        url
    }

    fn track(id: &str) -> RoomTrack {
        RoomTrack {
            id: id.to_owned(),
            title: id.to_owned(),
            artist: None,
            artist_id: None,
            album: None,
            duration: None,
            thumbnail_url: None,
            credits: Vec::new(),
        }
    }

    fn wait_for(
        updates: &Receiver<Update>,
        what: &str,
        mut matches: impl FnMut(&Snapshot) -> bool,
    ) -> Snapshot {
        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            if let Ok(Update::Snapshot { snapshot, .. }) =
                updates.recv_timeout(Duration::from_millis(200))
            {
                if matches(&snapshot) {
                    return snapshot;
                }
            }
        }
        panic!("timed out waiting for: {what}");
    }

    #[test]
    fn two_clients_stay_in_sync() {
        let url = start();
        let (a, a_updates) = Room::join(&url, "ABC234").unwrap();
        let (b, b_updates) = Room::join(&url, "abc-234").unwrap();
        wait_for(&a_updates, "a sees both members", |s| s.members == 2);

        a.send(Intent::PlayQueue {
            tracks: vec![track("one"), track("two"), track("three")],
            start: 1,
        });
        let seen = wait_for(&b_updates, "b gets the queue", |s| s.state.tracks.len() == 3);
        assert_eq!(seen.state.index, Some(1));
        assert!(seen.state.playing);

        b.send(Intent::Pause);
        let paused = wait_for(&a_updates, "a sees the pause", |s| !s.state.playing);
        assert_eq!(paused.state.index, Some(1));

        a.send(Intent::Seek {
            position_secs: 42.,
        });
        let sought = wait_for(&b_updates, "b sees the seek", |s| {
            s.state.position_secs == 42.
        });
        assert!(!sought.state.playing);
    }

    #[test]
    fn duplicate_track_end_reports_advance_once() {
        let url = start();
        let (a, a_updates) = Room::join(&url, "NEXT22").unwrap();
        let (b, _b_updates) = Room::join(&url, "NEXT22").unwrap();
        a.send(Intent::PlayQueue {
            tracks: vec![track("one"), track("two"), track("three")],
            start: 0,
        });
        wait_for(&a_updates, "queue loaded", |s| s.state.tracks.len() == 3);

        a.send(Intent::Next {
            expected_index: Some(0),
        });
        b.send(Intent::Next {
            expected_index: Some(0),
        });
        std::thread::sleep(Duration::from_millis(500));
        a.send(Intent::SetRepeat { repeat: true });
        let latest = wait_for(&a_updates, "repeat applied", |s| s.state.repeat);
        assert_eq!(latest.state.index, Some(1));
    }

    #[test]
    fn append_is_deduplicated_and_length_checked() {
        let url = start();
        let (a, a_updates) = Room::join(&url, "APND22").unwrap();
        let (b, _b_updates) = Room::join(&url, "APND22").unwrap();
        a.send(Intent::PlayQueue {
            tracks: vec![track("one")],
            start: 0,
        });
        wait_for(&a_updates, "queue loaded", |s| s.state.tracks.len() == 1);

        a.send(Intent::Append {
            tracks: vec![track("two"), track("one")],
            expected_len: 1,
        });
        let latest = wait_for(&a_updates, "append applied", |s| s.state.tracks.len() == 2);
        assert_eq!(latest.state.tracks[1].id, "two");

        b.send(Intent::Append {
            tracks: vec![track("three")],
            expected_len: 1,
        });
        a.send(Intent::SetRepeat { repeat: true });
        let after = wait_for(&a_updates, "barrier applied", |s| s.state.repeat);
        assert_eq!(after.state.tracks.len(), 2);
    }
}
