use crate::components::account::switcher::AccountSwitcher;
use gpui_kit::{
    App, Entity, IntoElement, RenderOnce, Styled, Window,
    assets::IconName,
    component::sidebar::{Sidebar, SidebarCollapsible, SidebarMenu, SidebarMenuItem},
    px,
};

#[derive(IntoElement)]
pub struct AppSidebar {
    collapsed: bool,
    account_switcher: Entity<AccountSwitcher>,
}

impl AppSidebar {
    pub fn new(collapsed: bool, account_switcher: Entity<AccountSwitcher>) -> Self {
        Self {
            collapsed,
            account_switcher,
        }
    }
}

impl RenderOnce for AppSidebar {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        Sidebar::new("sidebar")
            .collapsible(SidebarCollapsible::Offcanvas)
            .collapsed(self.collapsed)
            .w(px(240.))
            .header(self.account_switcher)
            .child(
                SidebarMenu::new().child(
                    SidebarMenuItem::new("Home")
                        .icon(IconName::House)
                        .active(true),
                ),
            )
    }
}
