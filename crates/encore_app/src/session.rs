use encore_core::{SettingKey, SettingsStore, Track};
use serde::{Deserialize, Serialize};

const SESSION: SettingKey = SettingKey::new("player", "session");
const HISTORY_KEPT: usize = 20;
const UPCOMING_KEPT: usize = 200;

#[derive(Serialize, Deserialize)]
struct SavedTrack {
    id: String,
    title: String,
    artist: Option<String>,
    artist_id: Option<String>,
    album: Option<String>,
    duration: Option<String>,
    thumbnail_url: Option<String>,
    credits: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct SavedSession {
    tracks: Vec<SavedTrack>,
    current: usize,
    pub position_secs: f64,
    pub duration_secs: f64,
    pub volume_percent: i64,
    pub shuffle: bool,
    pub repeat: bool,
}

pub struct Snapshot<'a> {
    pub tracks: &'a [Track],
    pub current: usize,
    pub position_secs: f64,
    pub duration_secs: f64,
    pub volume_percent: i64,
    pub shuffle: bool,
    pub repeat: bool,
}

impl SavedSession {
    pub fn from_snapshot(snapshot: Snapshot<'_>) -> Self {
        let start = snapshot.current.saturating_sub(HISTORY_KEPT);
        let end = (snapshot.current + UPCOMING_KEPT).min(snapshot.tracks.len());
        Self {
            tracks: snapshot.tracks[start..end]
                .iter()
                .map(|track| SavedTrack {
                    id: track.id.clone(),
                    title: track.title.clone(),
                    artist: track.artist.clone(),
                    artist_id: track.artist_id.clone(),
                    album: track.album.clone(),
                    duration: track.duration.clone(),
                    thumbnail_url: track.thumbnail_url.clone(),
                    credits: track.credits.clone(),
                })
                .collect(),
            current: snapshot.current - start,
            position_secs: snapshot.position_secs,
            duration_secs: snapshot.duration_secs,
            volume_percent: snapshot.volume_percent,
            shuffle: snapshot.shuffle,
            repeat: snapshot.repeat,
        }
    }

    pub fn current_index(&self) -> usize {
        self.current
    }

    pub fn into_tracks(self) -> Vec<Track> {
        self.tracks
            .into_iter()
            .map(|track| Track {
                id: track.id,
                title: track.title,
                artist: track.artist,
                artist_id: track.artist_id,
                album: track.album,
                duration: track.duration,
                thumbnail_url: track.thumbnail_url,
                credits: track.credits,
            })
            .collect()
    }

    pub fn is_playable(&self) -> bool {
        self.current < self.tracks.len()
    }
}

pub fn save(store: &impl SettingsStore, session: &SavedSession) -> anyhow::Result<()> {
    store.set_setting(SESSION, &serde_json::to_string(session)?)?;
    Ok(())
}

pub fn load(store: &impl SettingsStore) -> Option<SavedSession> {
    let raw = store.setting(SESSION).ok().flatten()?;
    serde_json::from_str(&raw).ok().filter(SavedSession::is_playable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use encore_storage::SqliteStore;

    fn track(id: &str) -> Track {
        Track {
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

    #[test]
    fn a_saved_session_is_restored_with_its_position() {
        let store = SqliteStore::open_in_memory().unwrap();
        let tracks: Vec<Track> = (0..50).map(|i| track(&i.to_string())).collect();
        let session = SavedSession::from_snapshot(Snapshot {
            tracks: &tracks,
            current: 30,
            position_secs: 42.5,
            duration_secs: 200.,
            volume_percent: 70,
            shuffle: true,
            repeat: false,
        });
        save(&store, &session).unwrap();

        let loaded = load(&store).unwrap();

        assert_eq!(loaded.position_secs, 42.5);
        let current = loaded.current_index();
        assert_eq!(loaded.into_tracks()[current].id, "30");
    }

    #[test]
    fn nothing_saved_restores_nothing() {
        let store = SqliteStore::open_in_memory().unwrap();

        assert!(load(&store).is_none());
    }
}
