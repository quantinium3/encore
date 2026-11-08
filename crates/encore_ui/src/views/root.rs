use std::time::Duration;

use crate::accounts::AccountsModel;
use crate::artist::ArtistModel;
use crate::components::account::switcher::AccountSwitcher;
use crate::components::artist::view::ArtistView;
use crate::components::layout::{header::Header, sidebar::AppSidebar};
use crate::components::player::queue_panel::{QueuePanel, QueuePanelState};
use crate::components::player::view::PlayerBarView;
use crate::components::playlist::detail::PlaylistDetailView;
use crate::components::room::panel::RoomPanel;
use crate::components::search::view::SearchView;
use crate::components::sidebar::panel::PlaylistsPanel;
use crate::navigation::{History, Location};
use crate::player::PlayerModel;
use crate::playlist_detail::PlaylistDetailModel;
use crate::playlists::PlaylistsModel;
use crate::room::RoomModel;
use crate::search::SearchModel;
use gpui_kit::{
    AppContext as _, Context, ElementId, Entity, IntoElement, ParentElement as _, Render,
    Styled as _, Subscription, Task, Window,
    assets::IconName,
    base::{h_flex, v_flex},
    component::{
        ActiveTheme as _, Disableable as _, Icon, Sizable as _, Theme, ThemeMode,
        button::{Button, ButtonVariants as _},
        input::{Input, InputEvent, InputState},
        scroll::ScrollableElement as _,
    },
    div, px,
    prelude::FluentBuilder as _,
};

const SEARCH_DEBOUNCE: Duration = Duration::from_millis(300);

pub struct DesktopRoot {
    history: History<Location>,
    search_debounce: Option<Task<()>>,
    sidebar_collapsed: bool,
    accounts: Entity<AccountsModel>,
    account_switcher: Entity<AccountSwitcher>,
    playlists: Entity<PlaylistsModel>,
    playlists_panel: Entity<PlaylistsPanel>,
    playlist_detail: Entity<PlaylistDetailModel>,
    playlist_detail_view: Entity<PlaylistDetailView>,
    artist: Entity<ArtistModel>,
    artist_view: Entity<ArtistView>,
    global_search: Entity<InputState>,
    search: Entity<SearchModel>,
    search_view: Entity<SearchView>,
    player: Entity<PlayerModel>,
    player_bar: Entity<PlayerBarView>,
    queue_state: Entity<QueuePanelState>,
    queue_panel: Entity<QueuePanel>,
    room: Entity<RoomModel>,
    room_panel: Entity<RoomPanel>,
    _subscriptions: Vec<Subscription>,
}

impl DesktopRoot {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let accounts = cx.new(|_| AccountsModel::default());
        let account_switcher = cx.new(|cx| AccountSwitcher::new(accounts.clone(), cx));
        let playlists = cx.new(|_| PlaylistsModel::default());
        let playlists_panel = cx.new(|cx| PlaylistsPanel::new(playlists.clone(), cx));
        let playlist_detail = cx.new(|_| PlaylistDetailModel::default());
        let player = cx.new(|_| PlayerModel::default());
        let playlist_search = cx.new(|cx| InputState::new(window, cx).placeholder("Search tracks"));
        let playlist_detail_view = cx.new(|cx| {
            PlaylistDetailView::new(playlist_detail.clone(), player.clone(), playlist_search, cx)
        });
        let artist = cx.new(|_| ArtistModel::default());
        cx.set_global(crate::artist::ArtistNavigator(artist.clone()));
        let artist_view =
            cx.new(|cx| ArtistView::new(artist.clone(), player.clone(), playlists.clone(), cx));
        let global_search = cx.new(|cx| InputState::new(window, cx).placeholder("Search songs"));
        let search = cx.new(|_| SearchModel::default());
        let search_view = cx.new(|cx| {
            SearchView::new(
                search.clone(),
                player.clone(),
                playlists.clone(),
                artist.clone(),
                cx,
            )
        });
        let queue_state = cx.new(|_| QueuePanelState::default());
        let queue_panel = cx.new(|cx| QueuePanel::new(player.clone(), queue_state.clone(), cx));
        let player_bar = cx.new(|cx| PlayerBarView::new(player.clone(), queue_state.clone(), cx));
        let room = cx.new(|_| RoomModel::default());
        let room_panel = cx.new(|cx| RoomPanel::new(room.clone(), window, cx));

        let subscriptions = vec![
            cx.observe(&room, |_, _, cx| cx.notify()),
            cx.observe(&playlists, {
                let search = search.clone();
                let artist = artist.clone();
                move |this, _, cx| {
                    search.update(cx, |model, cx| model.dismiss(cx));
                    artist.update(cx, |model, cx| model.clear(cx));
                    this.record_location(cx);
                }
            }),
            cx.observe(&artist, {
                let search = search.clone();
                move |this, _, cx| {
                    search.update(cx, |model, cx| model.dismiss(cx));
                    this.record_location(cx);
                }
            }),
            cx.observe(&search, |this, _, cx| {
                this.record_location(cx);
            }),
            cx.observe(&queue_state, |_, _, cx| cx.notify()),
            cx.subscribe(&global_search, {
                let search = search.clone();
                move |this: &mut Self, input, event: &InputEvent, cx| match event {
                    InputEvent::PressEnter { .. } => {
                        this.search_debounce = None;
                        let query = input.read(cx).value().trim().to_owned();
                        if !query.is_empty() {
                            search.update(cx, |model, cx| model.request(query.into(), cx));
                        }
                    }
                    InputEvent::Change => {
                        let query = input.read(cx).value().trim().to_owned();
                        if query.is_empty() {
                            this.search_debounce = None;
                            search.update(cx, |model, cx| model.dismiss(cx));
                            return;
                        }
                        let search = search.clone();
                        this.search_debounce = Some(cx.spawn(async move |_, cx| {
                            cx.background_executor().timer(SEARCH_DEBOUNCE).await;
                            search.update(cx, |model, cx| {
                                model.request_if_changed(query.into(), cx)
                            });
                        }));
                    }
                    _ => {}
                }
            }),
        ];

        Self {
            history: History::default(),
            search_debounce: None,
            sidebar_collapsed: false,
            accounts,
            account_switcher,
            playlists,
            playlists_panel,
            playlist_detail,
            playlist_detail_view,
            artist,
            artist_view,
            global_search,
            search,
            search_view,
            player,
            player_bar,
            queue_state,
            queue_panel,
            room,
            room_panel,
            _subscriptions: subscriptions,
        }
    }

    fn current_location(&self, cx: &Context<Self>) -> Location {
        let search = self.search.read(cx);
        if search.is_open() {
            return Location::Search {
                query: search.query().clone(),
                kind: search.kind(),
            };
        }
        match self.artist.read(cx).active() {
            Some(artist_id) => Location::Artist(artist_id.clone()),
            None => Location::Playlist(self.playlists.read(cx).active().cloned()),
        }
    }

    fn record_location(&mut self, cx: &mut Context<Self>) {
        let location = self.current_location(cx);
        let replaces = matches!(
            (self.history.current(), &location),
            (Some(Location::Search { .. }), Location::Search { .. })
        );
        match replaces {
            true => self.history.replace_current(location),
            false => self.history.record(location),
        }
        cx.notify();
    }

    fn go_back(&mut self, cx: &mut Context<Self>) {
        if let Some(location) = self.history.go_back() {
            self.show(location, cx);
        }
    }

    fn go_forward(&mut self, cx: &mut Context<Self>) {
        if let Some(location) = self.history.go_forward() {
            self.show(location, cx);
        }
    }

    fn show(&mut self, location: Location, cx: &mut Context<Self>) {
        match location {
            Location::Playlist(id) => {
                self.search.update(cx, |model, cx| model.dismiss(cx));
                self.artist.update(cx, |model, cx| model.clear(cx));
                if let Some(id) = id {
                    self.playlists.update(cx, |model, cx| model.select(id, cx));
                }
            }
            Location::Artist(id) => {
                self.search.update(cx, |model, cx| model.dismiss(cx));
                self.artist.update(cx, |model, cx| model.select(id, cx));
            }
            Location::Search { query, kind } => {
                self.search
                    .update(cx, |model, cx| model.request_with_kind(query, kind, cx));
            }
        }
        cx.notify();
    }

    pub fn accounts(&self) -> &Entity<AccountsModel> {
        &self.accounts
    }

    pub fn playlists(&self) -> &Entity<PlaylistsModel> {
        &self.playlists
    }

    pub fn playlist_detail(&self) -> &Entity<PlaylistDetailModel> {
        &self.playlist_detail
    }

    pub fn artist(&self) -> &Entity<ArtistModel> {
        &self.artist
    }

    pub fn search(&self) -> &Entity<SearchModel> {
        &self.search
    }

    pub fn room(&self) -> &Entity<RoomModel> {
        &self.room
    }

    pub fn player(&self) -> &Entity<PlayerModel> {
        &self.player
    }
}

impl Render for DesktopRoot {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let can_back = self.history.can_go_back();
        let can_forward = self.history.can_go_forward();
        let theme = cx.theme();
        let is_dark = theme.is_dark();
        let room_open = self.room.read(cx).is_open();
        v_flex()
            .size_full()
            .relative()
            .child(Header::new(
                self.sidebar_collapsed,
                cx.listener(|this, _, _, cx| {
                    this.sidebar_collapsed = !this.sidebar_collapsed;
                    cx.notify();
                }),
                room_open,
                cx.listener(|this, _, _, cx| {
                    this.room.update(cx, |model, cx| model.toggle_open(cx));
                }),
            ))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(AppSidebar::new(
                        self.sidebar_collapsed,
                        self.account_switcher.clone(),
                        self.playlists_panel.clone(),
                    ))
                    .child({
                        let search_open = self.search.read(cx).is_open();
                        let active_artist = match search_open {
                            true => None,
                            false => self.artist.read(cx).active().cloned(),
                        };
                        let (content, id) = match (search_open, &active_artist) {
                            (true, _) => (
                                self.search_view.clone().into_any_element(),
                                ElementId::from("main-content-search"),
                            ),
                            (false, Some(artist_id)) => (
                                self.artist_view.clone().into_any_element(),
                                format!("main-content-artist-{artist_id}").into(),
                            ),
                            (false, None) => (
                                self.playlist_detail_view.clone().into_any_element(),
                                "main-content-playlist".into(),
                            ),
                        };
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                h_flex()
                                    .w_full()
                                    .px_4()
                                    .pt_3()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        h_flex()
                                            .rounded(px(99.))
                                            .border_1()
                                            .border_color(theme.border)
                                            .child(
                                                Button::new("history-back")
                                                    .cursor_pointer()
                                                    .ghost()
                                                    .xsmall()
                                                    .disabled(!can_back)
                                                    .icon(Icon::new(IconName::ChevronLeft).size_4())
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.go_back(cx)
                                                    })),
                                            )
                                            .child(
                                                Button::new("history-forward")
                                                    .cursor_pointer()
                                                    .ghost()
                                                    .xsmall()
                                                    .disabled(!can_forward)
                                                    .icon(Icon::new(IconName::ChevronRight).size_4())
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.go_forward(cx)
                                                    })),
                                            ),
                                    )
                                    .child(
                                        h_flex()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                Input::new(&self.global_search)
                                                    .w(px(360.))
                                                    .cleanable(true)
                                                    .prefix(
                                                        Icon::new(IconName::Search)
                                                            .size_3p5()
                                                            .text_color(theme.muted_foreground),
                                                    ),
                                            )
                                            .child(
                                                Button::new("theme-toggle")
                                                    .cursor_pointer()
                                                    .ghost()
                                                    .small()
                                                    .icon(
                                                        Icon::new(match is_dark {
                                                            true => IconName::Sun,
                                                            false => IconName::Moon,
                                                        })
                                                        .size_4(),
                                                    )
                                                    .on_click(cx.listener(|_, _, window, cx| {
                                                        let mode = match Theme::global(cx).is_dark()
                                                        {
                                                            true => ThemeMode::Light,
                                                            false => ThemeMode::Dark,
                                                        };
                                                        Theme::change(mode, Some(window), cx);
                                                    })),
                                            ),
                                    ),
                            )
                            .child(content)
                            .overflow_y_scrollbar()
                            .id(id)
                    })
                    .when(self.queue_state.read(cx).is_open(), |this| {
                        this.child(self.queue_panel.clone())
                    }),
            )
            .child(self.player_bar.clone())
            .when(room_open, |this| {
                this.child(
                    div()
                        .absolute()
                        .top(px(44.))
                        .right(px(16.))
                        .child(self.room_panel.clone()),
                )
            })
    }
}
