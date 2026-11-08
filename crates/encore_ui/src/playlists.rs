use gpui_kit::{Context, SharedString};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistSummary {
    pub id: SharedString,
    pub title: SharedString,
    pub track_count: Option<usize>,
    pub thumbnail_url: Option<SharedString>,
    pub owner: Option<SharedString>,
}

#[derive(Default)]
pub struct PlaylistsModel {
    playlists: Vec<PlaylistSummary>,
    active: Option<SharedString>,
}

impl PlaylistsModel {
    pub fn playlists(&self) -> &[PlaylistSummary] {
        &self.playlists
    }

    pub fn active(&self) -> Option<&SharedString> {
        self.active.as_ref()
    }

    pub fn set_playlists(&mut self, playlists: Vec<PlaylistSummary>, cx: &mut Context<Self>) {
        self.playlists = playlists;
        self.active = self.resolved_active();
        cx.notify();
    }

    pub fn select(&mut self, id: SharedString, cx: &mut Context<Self>) {
        if self.is_new_selection(&id) {
            self.active = Some(id);
            cx.notify();
        }
    }

    fn resolved_active(&self) -> Option<SharedString> {
        let active = self.active.clone()?;
        match self.playlists.iter().any(|playlist| playlist.id == active) {
            true => Some(active),
            false => None,
        }
    }

    fn is_new_selection(&self, id: &SharedString) -> bool {
        self.active.as_ref() != Some(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playlist(id: &str) -> PlaylistSummary {
        PlaylistSummary {
            id: id.to_owned().into(),
            title: id.to_owned().into(),
            track_count: None,
            thumbnail_url: None,
            owner: None,
        }
    }

    fn id(raw: &str) -> SharedString {
        raw.to_owned().into()
    }

    fn model(ids: &[&str], active: Option<&str>) -> PlaylistsModel {
        PlaylistsModel {
            playlists: ids.iter().map(|id| playlist(id)).collect(),
            active: active.map(|id| id.to_owned().into()),
        }
    }

    #[test]
    fn a_fresh_model_has_no_playlists_and_nothing_active() {
        let model = PlaylistsModel::default();

        assert!(model.playlists().is_empty());
        assert!(model.active().is_none());
    }

    #[test]
    fn an_active_playlist_that_survives_a_refresh_stays_active() {
        let model = model(&["a", "b"], Some("b"));

        assert_eq!(model.resolved_active(), Some(id("b")));
    }

    #[test]
    fn an_active_playlist_that_drops_out_of_a_refresh_is_cleared() {
        let model = model(&["a"], Some("ghost"));

        assert_eq!(model.resolved_active(), None);
    }

    #[test]
    fn selecting_a_different_playlist_is_a_new_selection() {
        let model = model(&["a", "b"], Some("a"));

        assert!(model.is_new_selection(&id("b")));
    }

    #[test]
    fn selecting_the_already_active_playlist_is_not_a_new_selection() {
        let model = model(&["a"], Some("a"));

        assert!(!model.is_new_selection(&id("a")));
    }
}
