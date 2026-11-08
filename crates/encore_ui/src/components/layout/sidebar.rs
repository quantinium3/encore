use crate::components::account::switcher::AccountSwitcher;
use crate::components::sidebar::panel::PlaylistsPanel;
use gpui_kit::{
    App, ElementId, Entity, IntoElement, RenderOnce, Styled, Window,
    component::{
        Collapsible,
        sidebar::{Sidebar, SidebarCollapsible, SidebarItem},
    },
    px,
};

#[derive(Clone)]
struct PlaylistsSidebarContent(Entity<PlaylistsPanel>);

impl Collapsible for PlaylistsSidebarContent {
    fn collapsed(self, _collapsed: bool) -> Self {
        self
    }

    fn is_collapsed(&self) -> bool {
        false
    }
}

impl SidebarItem for PlaylistsSidebarContent {
    fn render(self, _id: impl Into<ElementId>, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        self.0
    }
}

#[derive(IntoElement)]
pub struct AppSidebar {
    collapsed: bool,
    account_switcher: Entity<AccountSwitcher>,
    playlists_panel: Entity<PlaylistsPanel>,
}

impl AppSidebar {
    pub fn new(
        collapsed: bool,
        account_switcher: Entity<AccountSwitcher>,
        playlists_panel: Entity<PlaylistsPanel>,
    ) -> Self {
        Self {
            collapsed,
            account_switcher,
            playlists_panel,
        }
    }
}

impl RenderOnce for AppSidebar {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        Sidebar::<PlaylistsSidebarContent>::new("sidebar")
            .collapsible(SidebarCollapsible::Offcanvas)
            .collapsed(self.collapsed)
            .w(px(240.))
            .header(self.account_switcher)
            .child(PlaylistsSidebarContent(self.playlists_panel))
    }
}
