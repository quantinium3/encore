use gpui_kit::{
    App, FontWeight, IntoElement, ObjectFit, ParentElement as _, RenderOnce, SharedString,
    Styled as _, StyledImage as _, Window,
    assets::IconName,
    base::{h_flex, v_flex},
    component::{ActiveTheme as _, Icon},
    div, img,
    prelude::FluentBuilder as _,
    px,
};

const POSTER_SIZE: f32 = 160.;

#[derive(IntoElement)]
pub struct PlaylistHeader {
    title: SharedString,
    subtitle: Option<SharedString>,
    meta: Option<SharedString>,
    description: Option<SharedString>,
    poster_url: Option<SharedString>,
    circular: bool,
}

impl PlaylistHeader {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            subtitle: None,
            meta: None,
            description: None,
            poster_url: None,
            circular: false,
        }
    }

    pub fn circular(mut self) -> Self {
        self.circular = true;
        self
    }

    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn meta(mut self, meta: impl Into<SharedString>) -> Self {
        self.meta = Some(meta.into());
        self
    }

    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn poster(mut self, url: impl Into<SharedString>) -> Self {
        self.poster_url = Some(url.into());
        self
    }
}

impl RenderOnce for PlaylistHeader {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;

        let poster = div()
            .size(px(POSTER_SIZE))
            .flex_shrink_0()
            .when(self.circular, |this| this.rounded_full())
            .when(!self.circular, |this| this.rounded(cx.theme().radius_lg))
            .overflow_hidden()
            .shadow_lg()
            .flex()
            .items_center()
            .justify_center()
            .bg(cx.theme().muted)
            .map(|this| match self.poster_url {
                Some(url) => {
                    this.child(img(url).size(px(POSTER_SIZE)).object_fit(ObjectFit::Cover))
                }
                None => this.child(Icon::new(IconName::ListMusic).size_8().text_color(muted)),
            });

        let text = v_flex()
            .flex_1()
            .min_w_0()
            .gap_1()
            .child(
                div()
                    .text_2xl()
                    .font_weight(FontWeight::BOLD)
                    .child(self.title),
            )
            .when_some(self.subtitle, |this, subtitle| {
                this.child(div().text_sm().text_color(muted).child(subtitle))
            })
            .when_some(self.meta, |this, meta| {
                this.child(div().text_sm().text_color(muted).child(meta))
            })
            .when_some(self.description, |this, description| {
                this.child(div().text_sm().text_color(muted).child(description))
            });

        h_flex()
            .w_full()
            .items_end()
            .gap_4()
            .child(poster)
            .child(text)
    }
}
