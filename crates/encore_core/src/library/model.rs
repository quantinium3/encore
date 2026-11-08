#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Playlist {
    pub id: String,
    pub title: String,
    pub track_count: Option<u32>,
    pub thumbnail_url: Option<String>,
    pub owner: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: Option<String>,
    pub artist_id: Option<String>,
    pub album: Option<String>,
    pub duration: Option<String>,
    pub thumbnail_url: Option<String>,
    pub credits: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaylistPrivacy {
    Public,
    Private,
    Unlisted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistDetail {
    pub title: String,
    pub description: Option<String>,
    pub thumbnail_url: Option<String>,
    pub track_count: Option<u32>,
    pub year: Option<String>,
    pub total_duration: Option<String>,
    pub privacy: Option<PlaylistPrivacy>,
    pub tracks: Vec<Track>,
}
