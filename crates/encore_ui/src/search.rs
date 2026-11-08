use gpui_kit::{Context, EventEmitter, SharedString};

use crate::artist::CardSummary;
use crate::playlist_detail::TrackSummary;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchKind {
    All,
    Songs,
    Videos,
    Albums,
    Artists,
    Playlists,
}

impl Default for SearchKind {
    fn default() -> Self {
        Self::All
    }
}

impl SearchKind {
    pub const ALL: [Self; 6] = [
        Self::All,
        Self::Songs,
        Self::Videos,
        Self::Albums,
        Self::Artists,
        Self::Playlists,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Songs => "Songs",
            Self::Videos => "Videos",
            Self::Albums => "Albums",
            Self::Artists => "Artists",
            Self::Playlists => "Playlists",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MixedItemSummary {
    Track(TrackSummary),
    Album(CardSummary),
    Artist(CardSummary),
    Playlist(CardSummary),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopResultSummary {
    pub item: MixedItemSummary,
    pub quick_picks: Vec<TrackSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MixedResultsSummary {
    pub top_result: Option<TopResultSummary>,
    pub items: Vec<MixedItemSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchEvent {
    Requested {
        query: SharedString,
        kind: SearchKind,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchState {
    Closed,
    Loading,
    Failed(SharedString),
    All(MixedResultsSummary),
    Songs(Vec<TrackSummary>),
    Videos(Vec<TrackSummary>),
    Albums(Vec<CardSummary>),
    Artists(Vec<CardSummary>),
    Playlists(Vec<CardSummary>),
}

pub struct SearchModel {
    query: SharedString,
    kind: SearchKind,
    state: SearchState,
}

impl Default for SearchModel {
    fn default() -> Self {
        Self {
            query: SharedString::default(),
            kind: SearchKind::default(),
            state: SearchState::Closed,
        }
    }
}

impl EventEmitter<SearchEvent> for SearchModel {}

impl SearchModel {
    pub fn query(&self) -> &SharedString {
        &self.query
    }

    pub fn kind(&self) -> SearchKind {
        self.kind
    }

    pub fn state(&self) -> &SearchState {
        &self.state
    }

    pub fn is_open(&self) -> bool {
        !matches!(self.state, SearchState::Closed)
    }

    pub fn request(&mut self, query: SharedString, cx: &mut Context<Self>) {
        self.query = query.clone();
        self.state = SearchState::Loading;
        cx.emit(SearchEvent::Requested {
            query,
            kind: self.kind,
        });
        cx.notify();
    }

    pub fn request_if_changed(&mut self, query: SharedString, cx: &mut Context<Self>) {
        if self.is_open() && self.query == query && !matches!(self.state, SearchState::Failed(_)) {
            return;
        }
        self.request(query, cx);
    }

    pub fn request_with_kind(
        &mut self,
        query: SharedString,
        kind: SearchKind,
        cx: &mut Context<Self>,
    ) {
        self.kind = kind;
        self.request(query, cx);
    }

    pub fn set_kind(&mut self, kind: SearchKind, cx: &mut Context<Self>) {
        if self.is_open() && self.kind != kind {
            self.kind = kind;
            self.state = SearchState::Loading;
            cx.emit(SearchEvent::Requested {
                query: self.query.clone(),
                kind,
            });
            cx.notify();
        }
    }

    pub fn set_results(
        &mut self,
        query: &SharedString,
        kind: SearchKind,
        state: SearchState,
        cx: &mut Context<Self>,
    ) {
        if self.accepts(query, kind) {
            self.state = state;
            cx.notify();
        }
    }

    pub fn set_failed(
        &mut self,
        query: &SharedString,
        kind: SearchKind,
        message: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) {
        if self.accepts(query, kind) {
            self.state = SearchState::Failed(message.into());
            cx.notify();
        }
    }

    pub fn dismiss(&mut self, cx: &mut Context<Self>) {
        if self.is_open() {
            self.query = SharedString::default();
            self.state = SearchState::Closed;
            cx.notify();
        }
    }

    fn accepts(&self, query: &SharedString, kind: SearchKind) -> bool {
        self.is_open() && &self.query == query && self.kind == kind
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(text: &str) -> SharedString {
        SharedString::from(text.to_owned())
    }

    #[test]
    fn a_fresh_model_is_closed_and_defaults_to_all() {
        let model = SearchModel::default();
        assert!(!model.is_open());
        assert_eq!(model.kind(), SearchKind::All);
    }

    #[test]
    fn results_for_a_superseded_query_are_not_accepted() {
        let mut model = SearchModel::default();
        model.query = query("new");
        model.state = SearchState::Loading;

        assert!(model.accepts(&query("new"), SearchKind::All));
        assert!(!model.accepts(&query("old"), SearchKind::All));
    }

    #[test]
    fn results_for_a_superseded_kind_are_not_accepted() {
        let mut model = SearchModel::default();
        model.query = query("new");
        model.kind = SearchKind::Albums;
        model.state = SearchState::Loading;

        assert!(model.accepts(&query("new"), SearchKind::Albums));
        assert!(!model.accepts(&query("new"), SearchKind::Songs));
    }

    #[test]
    fn results_are_not_accepted_once_closed() {
        let model = SearchModel::default();

        assert!(!model.accepts(&query(""), SearchKind::Songs));
    }
}
