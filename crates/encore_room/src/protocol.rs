use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomTrack {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(default)]
    pub artist_id: Option<String>,
    #[serde(default)]
    pub album: Option<String>,
    #[serde(default)]
    pub duration: Option<String>,
    #[serde(default)]
    pub thumbnail_url: Option<String>,
    #[serde(default)]
    pub credits: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoomState {
    pub rev: u64,
    pub tracks: Vec<RoomTrack>,
    pub index: Option<usize>,
    pub playing: bool,
    pub position_secs: f64,
    pub updated_at_ms: f64,
    pub repeat: bool,
}

impl RoomState {
    pub fn position_at(&self, server_now_ms: f64) -> f64 {
        match self.playing {
            true => (self.position_secs + (server_now_ms - self.updated_at_ms) / 1000.).max(0.),
            false => self.position_secs,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub state: RoomState,
    pub members: usize,
    pub server_now_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Intent {
    PlayQueue {
        tracks: Vec<RoomTrack>,
        start: usize,
    },
    Play,
    Pause,
    Seek {
        position_secs: f64,
    },
    Next {
        expected_index: Option<usize>,
    },
    Previous,
    Append {
        tracks: Vec<RoomTrack>,
        expected_len: usize,
    },
    SetRepeat {
        repeat: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(playing: bool) -> RoomState {
        RoomState {
            rev: 1,
            tracks: Vec::new(),
            index: Some(0),
            playing,
            position_secs: 10.,
            updated_at_ms: 1_000.,
            repeat: false,
        }
    }

    #[test]
    fn playing_position_advances_with_the_server_clock() {
        assert_eq!(state(true).position_at(3_500.), 12.5);
    }

    #[test]
    fn paused_position_is_frozen() {
        assert_eq!(state(false).position_at(9_000.), 10.);
    }

    #[test]
    fn intents_use_the_wire_shape_the_worker_expects() {
        let json = serde_json::to_value(Intent::Next {
            expected_index: Some(2),
        })
        .unwrap();
        assert_eq!(json["type"], "next");
        assert_eq!(json["expected_index"], 2);
        let json = serde_json::to_value(Intent::SetRepeat { repeat: true }).unwrap();
        assert_eq!(json["type"], "set_repeat");
    }

    #[test]
    fn snapshot_parses_the_worker_reply() {
        let body = r#"{"state":{"rev":4,"tracks":[{"id":"a","title":"T","artist":null}],
            "index":0,"playing":true,"position_secs":0,"updated_at_ms":5,"repeat":false},
            "members":2,"server_now_ms":9}"#;
        let snapshot: Snapshot = serde_json::from_str(body).unwrap();
        assert_eq!(snapshot.state.rev, 4);
        assert_eq!(snapshot.members, 2);
        assert_eq!(snapshot.state.tracks[0].title, "T");
    }
}
