use gpui_kit::{
    App, ClickEvent, ElementId, FontWeight, InteractiveElement as _, IntoElement, ObjectFit,
    ParentElement as _, RenderOnce, SharedString, StatefulInteractiveElement as _, Styled as _,
    StyledImage as _, Window,
    assets::IconName,
    base::v_flex,
    black,
    component::{ActiveTheme as _, Icon},
    div, img,
    prelude::FluentBuilder as _,
    px,
};

const CARD_SIZE: f32 = 168.;

#[derive(IntoElement)]
pub struct Card {
    id: ElementId,
    title: SharedString,
    subtitle: Option<SharedString>,
    thumbnail_url: Option<SharedString>,
    rounded_full: bool,
    on_click: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
}

impl Card {
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            subtitle: None,
            thumbnail_url: None,
            rounded_full: false,
            on_click: None,
        }
    }

    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn thumbnail(mut self, url: impl Into<SharedString>) -> Self {
        self.thumbnail_url = Some(url.into());
        self
    }

    pub fn rounded_full(mut self) -> Self {
        self.rounded_full = true;
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

impl RenderOnce for Card {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let foreground = cx.theme().foreground;
        let group = format!("{}-artwork", self.id);

        let artwork = div()
            .group(group.clone())
            .relative()
            .size(px(CARD_SIZE))
            .flex_shrink_0()
            .when(self.rounded_full, |this| this.rounded_full())
            .when(!self.rounded_full, |this| {
                this.rounded(cx.theme().radius_lg)
            })
            .overflow_hidden()
            .shadow_md()
            .flex()
            .items_center()
            .justify_center()
            .bg(cx.theme().muted)
            .map(|this| match self.thumbnail_url {
                Some(url) => this.child(img(url).size(px(CARD_SIZE)).object_fit(ObjectFit::Cover)),
                None => this.child(Icon::new(IconName::Music).size_8().text_color(muted)),
            })
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .invisible()
                    .bg(black().opacity(0.35))
                    .group_hover(group, |this| this.visible())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(44.))
                            .rounded_full()
                            .bg(gpui_kit::white())
                            .child(Icon::new(IconName::Play).size_4().text_color(black())),
                    ),
            );

        let text = v_flex()
            .w(px(CARD_SIZE))
            .mt_2()
            .gap_0p5()
            .when(self.rounded_full, |this| this.items_center().text_center())
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(foreground)
                    .line_clamp(2)
                    .child(self.title),
            )
            .when_some(self.subtitle, |this, subtitle| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .line_clamp(1)
                        .child(subtitle),
                )
            });

        let card = v_flex()
            .id(self.id)
            .w(px(CARD_SIZE))
            .flex_shrink_0()
            .cursor_pointer()
            .child(artwork)
            .child(text);

        match self.on_click {
            Some(handler) => card.on_click(move |event, window, cx| handler(event, window, cx)),
            None => card,
        }
    }
}
