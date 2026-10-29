use crate::components::layout::{header::Header, sidebar::AppSidebar};
use gpui_kit::{
    Context, IntoElement, ParentElement as _, Render, Styled as _, Window,
    base::{h_flex, v_flex},
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
                    .child(AppSidebar::new(self.sidebar_collapsed)),
            )
    }
}
