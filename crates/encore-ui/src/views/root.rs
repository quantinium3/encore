use crate::components::layout::header::Header;
use gpui_kit::{
    AppContext as _, Context, IntoElement, ParentElement as _, Render, Styled as _, Window,
    WindowOptions, base::v_flex, div,
};

pub struct DesktopRoot {
    sidebar_collapsed: bool,
}

impl DesktopRoot {
    pub fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self {
            sidebar_collapsed: false,
        }
    }
}

impl Render for DesktopRoot {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex().size_full().child(Header::new(
            self.sidebar_collapsed,
            cx.listener(|this, _, _, cx| {
                this.sidebar_collapsed = !this.sidebar_collapsed;
                cx.notify();
            }),
        ))
    }
}
