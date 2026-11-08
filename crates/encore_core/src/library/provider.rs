use thiserror::Error;

use super::{Album, Artist, ArtistPage, Playlist, PlaylistDetail, Track};
use crate::Session;

pub trait PlaylistProvider {
    fn playlists(&self, session: &Session) -> Result<Vec<Playlist>, PlaylistError>;

    fn playlist(
        &self,
        session: &Session,
        playlist_id: &str,
    ) -> Result<PlaylistDetail, PlaylistError>;
}

pub trait ArtistProvider {
    fn artist(&self, session: &Session, artist_id: &str) -> Result<ArtistPage, PlaylistError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchKind {
    All,
    Songs,
    Videos,
    Albums,
    Artists,
    Playlists,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MixedItem {
    Track(Track),
    Album(Album),
    Artist(Artist),
    Playlist(Playlist),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopResult {
    pub item: MixedItem,
    pub quick_picks: Vec<Track>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MixedResults {
    pub top_result: Option<TopResult>,
    pub items: Vec<MixedItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchResults {
    All(MixedResults),
    Songs(Vec<Track>),
    Videos(Vec<Track>),
    Albums(Vec<Album>),
    Artists(Vec<Artist>),
    Playlists(Vec<Playlist>),
}

pub trait SearchProvider {
    fn search(
        &self,
        session: &Session,
        kind: SearchKind,
        query: &str,
    ) -> Result<SearchResults, PlaylistError>;
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
