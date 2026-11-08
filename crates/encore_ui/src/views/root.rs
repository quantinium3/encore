use crate::accounts::AccountsModel;
use crate::artist::ArtistModel;
use crate::components::account::switcher::AccountSwitcher;
use crate::components::artist::view::ArtistView;
use crate::components::layout::{header::Header, sidebar::AppSidebar};
use crate::components::player::queue_panel::{QueuePanel, QueuePanelState};
use crate::components::player::view::PlayerBarView;
use crate::components::playlist::detail::PlaylistDetailView;
use crate::components::sidebar::panel::PlaylistsPanel;
use crate::player::PlayerModel;
use crate::playlist_detail::PlaylistDetailModel;
use crate::playlists::PlaylistsModel;
use gpui_kit::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, Styled as _, Subscription, Window,
    base::{h_flex, v_flex},
    component::{input::InputState, scroll::ScrollableElement as _},
    prelude::FluentBuilder as _,
};

pub struct DesktopRoot {
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

        let subscriptions = vec![cx.observe(&playlists, {
            let artist = artist.clone();
            move |_, _, cx| {
                artist.update(cx, |model, cx| model.clear(cx));
            }
        })];

        Self {
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
                        h_flex()
                            .id(id)
                            .flex_1()
                            .min_w_0()
                            .child(content)
                            .overflow_y_scrollbar()
                    })
                    .when(self.queue_state.read(cx).is_open(), |this| {
                        this.child(self.queue_panel.clone())
                    }),
            )
            .child(self.player_bar.clone())
    }
}
