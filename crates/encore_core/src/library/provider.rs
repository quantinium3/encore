use thiserror::Error;

use super::{Playlist, PlaylistDetail, Track};
use crate::Session;

pub trait PlaylistProvider {
    fn playlists(&self, session: &Session) -> Result<Vec<Playlist>, PlaylistError>;

    fn playlist(
        &self,
        session: &Session,
        playlist_id: &str,
    ) -> Result<PlaylistDetail, PlaylistError>;
}

pub trait RadioProvider {
    fn radio(&self, session: &Session, video_id: &str) -> Result<Vec<Track>, PlaylistError>;
}

#[derive(Debug, Error)]
pub enum PlaylistError {
    #[error("the session was rejected")]
    Rejected,
    #[error("could not fetch playlists: {0}")]
    Backend(#[source] Box<dyn std::error::Error + Send + Sync>),
}
