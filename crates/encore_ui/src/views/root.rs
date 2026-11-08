use crate::accounts::AccountsModel;
use crate::components::account::switcher::AccountSwitcher;
use crate::components::layout::{header::Header, sidebar::AppSidebar};
use crate::components::playlist::detail::PlaylistDetailView;
use crate::components::sidebar::panel::PlaylistsPanel;
use crate::playlist_detail::PlaylistDetailModel;
use crate::playlists::PlaylistsModel;
use gpui_kit::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, Styled as _, Window,
    base::{h_flex, v_flex},
    component::{input::InputState, scroll::ScrollableElement as _},
};

pub struct DesktopRoot {
    sidebar_collapsed: bool,
    accounts: Entity<AccountsModel>,
    account_switcher: Entity<AccountSwitcher>,
    playlists: Entity<PlaylistsModel>,
    playlists_panel: Entity<PlaylistsPanel>,
    playlist_detail: Entity<PlaylistDetailModel>,
    playlist_detail_view: Entity<PlaylistDetailView>,
}

impl DesktopRoot {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let accounts = cx.new(|_| AccountsModel::default());
        let account_switcher = cx.new(|cx| AccountSwitcher::new(accounts.clone(), cx));
        let playlists = cx.new(|_| PlaylistsModel::default());
        let playlists_panel = cx.new(|cx| PlaylistsPanel::new(playlists.clone(), cx));
        let playlist_detail = cx.new(|_| PlaylistDetailModel::default());
        let playlist_search = cx.new(|cx| InputState::new(window, cx).placeholder("Search tracks"));
        let playlist_detail_view = cx.new(|cx| {
            PlaylistDetailView::new(playlist_detail.clone(), playlist_search, cx)
        });
        Self {
            sidebar_collapsed: false,
            accounts,
            account_switcher,
            playlists,
            playlists_panel,
            playlist_detail,
            playlist_detail_view,
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
}

impl Render for DesktopRoot {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                    .child(
                        h_flex()
                            .id("main-content")
                            .flex_1()
                            .min_w_0()
                            .child(self.playlist_detail_view.clone())
                            .overflow_y_scrollbar(),
                    ),
            )
    }
}
