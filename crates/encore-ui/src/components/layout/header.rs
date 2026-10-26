use gpui_kit::*;
use gpui_kit::{
    App, ClickEvent, IntoElement, RenderOnce, Window,
    component::{TitleBar, sidebar::SidebarToggleButton},
};

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Header {
    sidebar_collapsed: bool,
    on_toggle_sidebar: ClickHandler,
}

impl Header {
    pub fn new(
        sidebar_collapsed: bool,
        on_toggle_sidebar: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            sidebar_collapsed,
            on_toggle_sidebar: Box::new(on_toggle_sidebar),
        }
    }
}

impl RenderOnce for Header {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        TitleBar::new().child(
            SidebarToggleButton::new()
                .collapsed(self.sidebar_collapsed)
                .accessibility_label("Toggle Sidebar")
                .on_click(self.on_toggle_sidebar),
        )
    }
}
