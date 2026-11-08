use crate::accounts::AccountsModel;
use crate::artist::ArtistModel;
use crate::components::account::switcher::AccountSwitcher;
use crate::components::artist::view::ArtistView;
use crate::components::layout::{header::Header, sidebar::AppSidebar};
use crate::components::player::queue_panel::{QueuePanel, QueuePanelState};
use crate::components::player::view::PlayerBarView;
use crate::components::playlist::detail::PlaylistDetailView;
use crate::components::sidebar::panel::PlaylistsPanel;
use crate::navigation::{History, Location};
use crate::player::PlayerModel;
use crate::playlist_detail::PlaylistDetailModel;
use crate::playlists::PlaylistsModel;
use gpui_kit::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, Styled as _, Subscription, Window,
    assets::IconName,
    base::{h_flex, v_flex},
    component::{
        ActiveTheme as _, Disableable as _, Icon,
        button::{Button, ButtonVariants as _},
        input::InputState,
        scroll::ScrollableElement as _,
    },
    div, px,
    prelude::FluentBuilder as _,
};

pub struct DesktopRoot {
    history: History<Location>,
    sidebar_collapsed: bool,
    accounts: Entity<AccountsModel>,
    account_switcher: Entity<AccountSwitcher>,
    playlists: Entity<PlaylistsModel>,
    playlists_panel: Entity<PlaylistsPanel>,
    playlist_detail: Entity<PlaylistDetailModel>,
    playlist_detail_view: Entity<PlaylistDetailView>,
    artist: Entity<ArtistModel>,
    artist_view: Entity<ArtistView>,
    player: Entity<PlayerModel>,
    player_bar: Entity<PlayerBarView>,
    queue_state: Entity<QueuePanelState>,
    queue_panel: Entity<QueuePanel>,
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
        let queue_state = cx.new(|_| QueuePanelState::default());
        let queue_panel = cx.new(|cx| QueuePanel::new(player.clone(), queue_state.clone(), cx));
        let player_bar = cx.new(|cx| PlayerBarView::new(player.clone(), queue_state.clone(), cx));

        let subscriptions = vec![
            cx.observe(&playlists, {
                let artist = artist.clone();
                move |this, _, cx| {
                    artist.update(cx, |model, cx| model.clear(cx));
                    this.record_location(cx);
                }
            }),
            cx.observe(&artist, |this, _, cx| {
                this.record_location(cx);
            }),
        ];

        Self {
            history: History::default(),
            sidebar_collapsed: false,
            accounts,
            account_switcher,
            playlists,
            playlists_panel,
            playlist_detail,
            playlist_detail_view,
            artist,
            artist_view,
            player,
            player_bar,
            queue_state,
            queue_panel,
            _subscriptions: subscriptions,
        }
    }

    fn current_location(&self, cx: &Context<Self>) -> Location {
        match self.artist.read(cx).active() {
            Some(artist_id) => Location::Artist(artist_id.clone()),
            None => Location::Playlist(self.playlists.read(cx).active().cloned()),
        }
    }

    fn record_location(&mut self, cx: &mut Context<Self>) {
        let location = self.current_location(cx);
        self.history.record(location);
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
                self.artist.update(cx, |model, cx| model.clear(cx));
                if let Some(id) = id {
                    self.playlists.update(cx, |model, cx| model.select(id, cx));
                }
            }
            Location::Artist(id) => {
                self.artist.update(cx, |model, cx| model.select(id, cx));
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

    pub fn player(&self) -> &Entity<PlayerModel> {
        &self.player
    }
}

impl Render for DesktopRoot {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active_artist = self.artist.read(cx).active().is_some();
        let can_back = self.history.can_go_back();
        let can_forward = self.history.can_go_forward();
        let theme = cx.theme();
        v_flex()
            .size_full()
            .child(Header::new(
                self.sidebar_collapsed,
                cx.listener(|this, _, _, cx| {
                    this.sidebar_collapsed = !this.sidebar_collapsed;
                    cx.notify();
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
                        let (content, id) = match active_artist {
                            true => (
                                self.artist_view.clone().into_any_element(),
                                "main-content-artist",
                            ),
                            false => (
                                self.playlist_detail_view.clone().into_any_element(),
                                "main-content-playlist",
                            ),
                        };
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                h_flex().w_full().px_4().pt_3().items_center().child(
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
    }
}
