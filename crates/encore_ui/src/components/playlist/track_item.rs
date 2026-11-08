use std::time::Duration;

use gpui_kit::{
    Animation, AnimationExt as _, AnyElement, App, ClickEvent, ElementId, FontWeight, Hsla,
    InteractiveElement as _, IntoElement, MouseButton, ObjectFit, StatefulInteractiveElement as _, ParentElement as _, RenderOnce, SharedString,
    Styled as _, StyledImage as _, Window,
    assets::IconName,
    base::{h_flex, v_flex},
    component::{
        ActiveTheme as _, Icon, Selectable as _, Sizable as _, Size,
        button::{Button, ButtonVariants as _},
        spinner::Spinner,
    },
    div, ease_in_out, img,
    prelude::FluentBuilder as _,
    px,
};

use crate::components::marquee::Marquee;

const THUMBNAIL_SIZE: f32 = 40.;
const INDEX_WIDTH: f32 = 24.;
const INDEX_SLOT_HEIGHT: f32 = 20.;

const ROW_GROUP: &str = "track-row";

#[derive(IntoElement)]
pub struct PlaylistTrackItem {
    id: ElementId,
    index: Option<usize>,
    overlay: bool,
    title: SharedString,
    artist: Option<SharedString>,
    duration: Option<SharedString>,
    thumbnail_url: Option<SharedString>,
    active: bool,
    loading: bool,
    playing: bool,
    on_click: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
    on_artist_click: Option<Box<dyn Fn(&mut App) + 'static>>,
}

impl PlaylistTrackItem {
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            index: None,
            overlay: false,
            title: title.into(),
            artist: None,
            duration: None,
            thumbnail_url: None,
            active: false,
            loading: false,
            playing: false,
            on_click: None,
            on_artist_click: None,
        }
    }

    pub fn index(mut self, index: usize) -> Self {
        self.index = Some(index);
        self
    }

    pub fn overlay(mut self) -> Self {
        self.overlay = true;
        self
    }

    pub fn artist(mut self, artist: impl Into<SharedString>) -> Self {
        self.artist = Some(artist.into());
        self
    }

    pub fn duration(mut self, duration: impl Into<SharedString>) -> Self {
        self.duration = Some(duration.into());
        self
    }

    pub fn thumbnail(mut self, url: impl Into<SharedString>) -> Self {
        self.thumbnail_url = Some(url.into());
        self
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    pub fn playing(mut self, playing: bool) -> Self {
        self.playing = playing;
        self
    }

    pub fn on_artist_click(mut self, handler: impl Fn(&mut App) + 'static) -> Self {
        self.on_artist_click = Some(Box::new(handler));
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

impl RenderOnce for PlaylistTrackItem {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let foreground = cx.theme().foreground;
        let accent = cx.theme().primary;

        let index = match self.overlay {
            true => None,
            false => Some(render_index(
                &self.id,
                self.index,
                self.loading,
                self.playing,
                muted,
                foreground,
                accent,
            )),
        };
        let overlay = self.overlay.then(|| {
            thumbnail_overlay(
                &self.id,
                self.loading,
                self.playing,
                self.active,
                (accent, cx.theme().primary_foreground),
            )
        });

        let thumbnail = div()
            .size(px(THUMBNAIL_SIZE))
            .flex_shrink_0()
            .relative()
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
                None => this.child(Icon::new(IconName::Music).size_4().text_color(muted)),
            })
            .children(overlay);

        let title_color = match self.active {
            true => accent,
            false => foreground,
        };
        let text = v_flex()
            .flex_1()
            .min_w_0()
            .child(Marquee::new(
                format!("{}-title", self.id),
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(title_color)
                    .child(self.title),
            ))
            .when_some(self.artist, |this, artist| match self.on_artist_click {
                Some(handler) => this.child(
                    h_flex().w_full().min_w_0().child(
                        div()
                            .id(format!("{}-artist-link", self.id))
                            .max_w_full()
                            .text_color(muted)
                            .cursor_pointer()
                            .hover(|style| style.text_color(foreground).underline())
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                handler(cx);
                            })
                            .child(
                                Marquee::new(
                                    format!("{}-artist", self.id),
                                    div().text_xs().child(artist),
                                )
                                .fit_content(),
                            ),
                    ),
                ),
                None => this.child(Marquee::new(
                    format!("{}-artist", self.id),
                    div().text_xs().text_color(muted).child(artist),
                )),
            });

        let content = h_flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_x_3()
            .children(index)
            .child(thumbnail)
            .child(text)
            .when_some(self.duration, |this, duration| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .text_xs()
                        .text_color(muted)
                        .child(duration),
                )
            });

        let button = Button::new(self.id)

            .cursor_pointer()
            .ghost()
            .selected(self.active)
            .group(ROW_GROUP)
            .w_full()
            .h(px(56.))
            .px_2()
            .child(content);

        match self.on_click {
            Some(handler) => button.on_click(move |event, window, cx| handler(event, window, cx)),
            None => button,
        }
    }
}

fn render_index(
    row_id: &ElementId,
    index: Option<usize>,
    loading: bool,
    playing: bool,
    muted: Hsla,
    foreground: Hsla,
    accent: Hsla,
) -> impl IntoElement {
    let slot = div()
        .w(px(INDEX_WIDTH))
        .h(px(INDEX_SLOT_HEIGHT))
        .flex_shrink_0()
        .relative()
        .flex()
        .items_center()
        .justify_center()
        .text_sm()
        .text_color(muted);

    if loading {
        return slot.child(Spinner::new().with_size(Size::XSmall).color(muted));
    }

    let base: AnyElement = match (playing, index) {
        (true, _) => now_playing_indicator(row_id, accent).into_any_element(),
        (false, Some(index)) => div().child(index.to_string()).into_any_element(),
        (false, None) => div().into_any_element(),
    };
    let hover_icon = match playing {
        true => IconName::Pause,
        false => IconName::Play,
    };

    slot.child(
        div()
            .group_hover(ROW_GROUP, |style| style.opacity(0.))
            .child(base),
    )
    .child(
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .opacity(0.)
            .group_hover(ROW_GROUP, |style| style.opacity(1.))
            .child(Icon::new(hover_icon).size_4().text_color(foreground)),
    )
}

fn thumbnail_overlay(
    row_id: &ElementId,
    loading: bool,
    playing: bool,
    active: bool,
    (button_bg, button_fg): (Hsla, Hsla),
) -> impl IntoElement {
    let white = gpui_kit::white();
    let busy = loading || playing;
    let hover_icon = match playing {
        true => IconName::Pause,
        false => IconName::Play,
    };
    let status: AnyElement = match (loading, playing) {
        (true, _) => Spinner::new()
            .with_size(Size::XSmall)
            .color(white)
            .into_any_element(),
        (false, true) => now_playing_indicator(row_id, white).into_any_element(),
        (false, false) => div().into_any_element(),
    };
    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(gpui_kit::black().opacity(match (busy, active) {
            (true, _) => 0.55,
            (false, true) => 0.35,
            _ => 0.,
        }))
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .group_hover(ROW_GROUP, |style| style.opacity(0.))
                .child(status),
        )
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .opacity(0.)
                .bg(gpui_kit::black().opacity(0.35))
                .group_hover(ROW_GROUP, |style| style.opacity(1.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(26.))
                        .rounded_full()
                        .bg(button_bg)
                        .child(
                            Icon::new(hover_icon)
                                .size_3p5()
                                .text_color(button_fg),
                        ),
                ),
        )
}

const EQUALIZER_MIN_HEIGHT: f32 = 4.;
const EQUALIZER_SPAN: f32 = 8.;
const EQUALIZER_BAR_DURATIONS_MS: [u64; 3] = [480, 620, 400];

fn now_playing_indicator(row_id: &ElementId, color: Hsla) -> impl IntoElement {
    h_flex()
        .items_end()
        .justify_center()
        .gap(px(2.))
        .h(px(EQUALIZER_MIN_HEIGHT + EQUALIZER_SPAN))
        .children(
            EQUALIZER_BAR_DURATIONS_MS
                .into_iter()
                .enumerate()
                .map(|(bar, duration_ms)| {
                    equalizer_bar(format!("{row_id}-eq-{bar}"), color, duration_ms)
                }),
        )
}

fn equalizer_bar(id: impl Into<SharedString>, color: Hsla, duration_ms: u64) -> impl IntoElement {
    div()
        .w(px(3.))
        .rounded(px(1.))
        .bg(color)
        .h(px(EQUALIZER_MIN_HEIGHT))
        .with_animation(
            id.into(),
            Animation::new(Duration::from_millis(duration_ms))
                .repeat()
                .with_easing(ease_in_out),
            |this, delta| {
                let height =
                    EQUALIZER_MIN_HEIGHT + (delta * std::f32::consts::PI).sin() * EQUALIZER_SPAN;
                this.h(px(height))
            },
        )
}
