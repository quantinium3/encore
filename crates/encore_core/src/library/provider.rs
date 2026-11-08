use thiserror::Error;

use super::{Playlist, PlaylistDetail};
use crate::Session;

pub trait PlaylistProvider {
    fn playlists(&self, session: &Session) -> Result<Vec<Playlist>, PlaylistError>;

    fn playlist(
        &self,
        session: &Session,
        playlist_id: &str,
    ) -> Result<PlaylistDetail, PlaylistError>;
}

#[derive(Debug, Error)]
pub enum PlaylistError {
    #[error("the session was rejected")]
    Rejected,
    #[error("could not fetch playlists: {0}")]
    Backend(#[source] Box<dyn std::error::Error + Send + Sync>),
}
