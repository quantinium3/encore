use gpui_kit::{Context, Entity, Global, SharedString};

use crate::playlist_detail::TrackSummary;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardSummary {
    pub id: SharedString,
    pub title: SharedString,
    pub subtitle: Option<SharedString>,
    pub thumbnail_url: Option<SharedString>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtistSummary {
    pub artist_id: SharedString,
    pub name: SharedString,
    pub description: Option<SharedString>,
    pub subscribers: Option<SharedString>,
    pub header_image_url: Option<SharedString>,
    pub tracks: Vec<TrackSummary>,
    pub tracks_playlist_id: Option<SharedString>,
    pub albums: Vec<CardSummary>,
    pub singles: Vec<CardSummary>,
    pub similar_artists: Vec<CardSummary>,
}

pub struct ArtistNavigator(pub Entity<ArtistModel>);

impl Global for ArtistNavigator {}

#[derive(Default)]
pub struct ArtistModel {
    active: Option<SharedString>,
    page: Option<ArtistSummary>,
}

impl ArtistModel {
    pub fn active(&self) -> Option<&SharedString> {
        self.active.as_ref()
    }

    pub fn page(&self) -> Option<&ArtistSummary> {
        self.page.as_ref()
    }

    pub fn select(&mut self, artist_id: SharedString, cx: &mut Context<Self>) {
        if self.is_new_selection(&artist_id) {
            self.active = Some(artist_id);
            self.page = None;
            cx.notify();
        }
    }

    pub fn set_page(&mut self, page: Option<ArtistSummary>, cx: &mut Context<Self>) {
        if self.accepts(&page) {
            self.page = page;
            cx.notify();
        }
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        if self.active.is_some() {
            self.active = None;
            self.page = None;
            cx.notify();
        }
    }

    fn is_new_selection(&self, artist_id: &SharedString) -> bool {
        self.active.as_ref() != Some(artist_id)
    }

    fn accepts(&self, page: &Option<ArtistSummary>) -> bool {
        match page {
            None => true,
            Some(page) => Some(&page.artist_id) == self.active.as_ref(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(text: &str) -> SharedString {
        SharedString::from(text.to_owned())
    }

    fn page(artist_id: &str) -> ArtistSummary {
        ArtistSummary {
            artist_id: id(artist_id),
            name: id(artist_id),
            description: None,
            subscribers: None,
            header_image_url: None,
            tracks: Vec::new(),
            tracks_playlist_id: None,
            albums: Vec::new(),
            singles: Vec::new(),
            similar_artists: Vec::new(),
        }
    }

    #[test]
    fn a_fresh_model_has_no_active_artist() {
        let model = ArtistModel::default();
        assert!(model.active().is_none());
        assert!(model.page().is_none());
    }

    #[test]
    fn selecting_a_different_artist_is_a_new_selection() {
        let model = ArtistModel {
            active: Some(id("a")),
            page: Some(page("a")),
        };

        assert!(model.is_new_selection(&id("b")));
    }

    #[test]
    fn selecting_the_already_active_artist_is_not_a_new_selection() {
        let model = ArtistModel {
            active: Some(id("a")),
            page: Some(page("a")),
        };

        assert!(!model.is_new_selection(&id("a")));
    }

    #[test]
    fn a_page_for_the_active_artist_is_accepted() {
        let model = ArtistModel {
            active: Some(id("a")),
            page: None,
        };

        assert!(model.accepts(&Some(page("a"))));
    }

    #[test]
    fn a_page_for_a_superseded_artist_is_not_accepted() {
        let model = ArtistModel {
            active: Some(id("b")),
            page: None,
        };

        assert!(!model.accepts(&Some(page("a"))));
    }

    #[test]
    fn clearing_the_page_is_always_accepted() {
        let model = ArtistModel {
            active: Some(id("a")),
            page: Some(page("a")),
        };

        assert!(model.accepts(&None));
    }
}
