use gpui_kit::{
    App, ClickEvent, ElementId, FontWeight, IntoElement, ObjectFit, ParentElement as _, RenderOnce,
    SharedString, Styled as _, StyledImage as _, Window,
    assets::IconName,
    base::{h_flex, v_flex},
    component::{
        ActiveTheme as _, Icon, Selectable as _,
        button::{Button, ButtonVariants as _},
    },
    div, img,
    prelude::FluentBuilder as _,
    px,
};

use crate::components::marquee::Marquee;

const THUMBNAIL_SIZE: f32 = 36.;

#[derive(IntoElement)]
pub struct SidebarPlaylistItem {
    id: ElementId,
    marquee_id: SharedString,
    title: SharedString,
    subtitle: Option<SharedString>,
    thumbnail_url: Option<SharedString>,
    active: bool,
    on_click: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
}

impl SidebarPlaylistItem {
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        let id = id.into();
        let marquee_id = format!("{:?}-title", id).into();
        Self {
            id: id.into(),
            marquee_id: marquee_id,
            title: title.into(),
            subtitle: None,
            thumbnail_url: None,
            active: false,
            on_click: None,
        }
    }

    pub fn thumbnail(mut self, url: impl Into<SharedString>) -> Self {
        self.thumbnail_url = Some(url.into());
        self
    }

    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for SidebarPlaylistItem {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let foreground = cx.theme().foreground;

        let thumbnail = div()
            .size(px(THUMBNAIL_SIZE))
            .flex_shrink_0()
            .rounded(px(4.))
            .overflow_hidden()
            .flex()
            .items_center()
            .justify_center()
            .bg(cx.theme().muted)
            .map(|this| match self.thumbnail_url {
                Some(url) => this.child(
                    img(url)
                        .size(px(THUMBNAIL_SIZE))
                        .object_fit(ObjectFit::Cover),
                ),
                None => this.child(Icon::new(IconName::ListMusic).size_4().text_color(muted)),
            });

        let text = v_flex()
            .flex_1()
            .min_w_0()
            .items_start()
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(foreground)
                    .child(Marquee::new(self.marquee_id, self.title)),
            )
            .when_some(self.subtitle, |this, subtitle| {
                this.child(
                    div()
                        .w_full()
                        .min_w_0()
                        .text_xs()
                        .text_color(muted)
                        .line_clamp(1)
                        .child(subtitle),
                )
            });

        let content = h_flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_x_3()
            .child(thumbnail)
            .child(text);

        let button = Button::new(self.id)
            .cursor_pointer()
            .ghost()
            .selected(self.active)
            .w_full()
            .h(px(52.))
            .px_3()
            .child(content);

        match self.on_click {
            Some(handler) => button.on_click(move |event, window, cx| handler(event, window, cx)),
            None => button,
        }
    }
}
