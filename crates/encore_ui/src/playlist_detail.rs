use gpui_kit::{Context, SharedString};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackSummary {
    pub id: SharedString,
    pub title: SharedString,
    pub artist: Option<SharedString>,
    pub artist_id: Option<SharedString>,
    pub album: Option<SharedString>,
    pub duration: Option<SharedString>,
    pub thumbnail_url: Option<SharedString>,
    pub credits: Vec<SharedString>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaylistPrivacy {
    Public,
    Private,
    Unlisted,
}

impl PlaylistPrivacy {
    pub fn label(self) -> &'static str {
        match self {
            Self::Public => "Public",
            Self::Private => "Private",
            Self::Unlisted => "Unlisted",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistDetailSummary {
    pub playlist_id: SharedString,
    pub title: SharedString,
    pub description: Option<SharedString>,
    pub thumbnail_url: Option<SharedString>,
    pub track_count: Option<usize>,
    pub year: Option<SharedString>,
    pub total_duration: Option<SharedString>,
    pub privacy: Option<PlaylistPrivacy>,
    pub tracks: Vec<TrackSummary>,
}

#[derive(Default)]
pub struct PlaylistDetailModel {
    detail: Option<PlaylistDetailSummary>,
}

impl PlaylistDetailModel {
    pub fn detail(&self) -> Option<&PlaylistDetailSummary> {
        self.detail.as_ref()
    }

    pub fn set_detail(&mut self, detail: Option<PlaylistDetailSummary>, cx: &mut Context<Self>) {
        self.detail = detail;
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_model_has_no_detail() {
        let model = PlaylistDetailModel::default();

        assert!(model.detail().is_none());
    }
}
