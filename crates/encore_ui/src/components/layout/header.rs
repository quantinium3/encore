use gpui_kit::*;
use gpui_kit::{
    App, ClickEvent, IntoElement, RenderOnce, Window,
    assets::IconName,
    base::h_flex,
    component::{
        Icon, Selectable as _, Sizable as _, TitleBar, sidebar::SidebarToggleButton,
        button::{Button, ButtonVariants as _},
    },
};

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Header {
    sidebar_collapsed: bool,
    on_toggle_sidebar: ClickHandler,
    room_active: bool,
    on_toggle_room: ClickHandler,
}

impl Header {
    pub fn new(
        sidebar_collapsed: bool,
        on_toggle_sidebar: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
        room_active: bool,
        on_toggle_room: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            sidebar_collapsed,
            on_toggle_sidebar: Box::new(on_toggle_sidebar),
            room_active,
            on_toggle_room: Box::new(on_toggle_room),
        }
    }
}

impl RenderOnce for Header {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let sidebar_toggle = div()
            .id("sidebar-toggle")
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(
                SidebarToggleButton::new()
                    .collapsed(self.sidebar_collapsed)
                    .accessibility_label("Toggle Sidebar")
                    .on_click(self.on_toggle_sidebar),
            );

        let room = div()
            .id("room-toggle")
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(
                Button::new("room-toggle-button")
                    .cursor_pointer()
                    .ghost()
                    .small()
                    .selected(self.room_active)
                    .icon(Icon::new(IconName::Users).size_4())
                    .label("Listen together")
                    .on_click(self.on_toggle_room),
            );

        TitleBar::new().child(
            h_flex()
                .flex_1()
                .items_center()
                .justify_between()
                .child(sidebar_toggle)
                .child(room),
        )
    }
}
