use gpui_kit::{
    App, IntoElement, RenderOnce, Styled, Window,
    assets::IconName,
    component::sidebar::{Sidebar, SidebarCollapsible, SidebarMenu, SidebarMenuItem},
    px,
};

#[derive(IntoElement)]
pub struct AppSidebar {
    collapsed: bool,
}

impl AppSidebar {
    pub fn new(collapsed: bool) -> Self {
        Self { collapsed }
    }
}

impl RenderOnce for AppSidebar {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        Sidebar::new("sidebar")
            .collapsible(SidebarCollapsible::Offcanvas)
            .collapsed(self.collapsed)
            .w(px(240.))
            .child(
                SidebarMenu::new().child(
                    SidebarMenuItem::new("Home")
                        .icon(IconName::House)
                        .active(true),
                ),
            )
    }
}
