use gpui_kit::{
    App, ClickEvent, Entity, FontWeight, Hsla, InteractiveElement as _, IntoElement, ObjectFit,
    ParentElement as _, Pixels, RenderOnce, SharedString, StatefulInteractiveElement as _,
    Styled as _, StyledImage as _, Window,
    assets::IconName,
    base::{h_flex, v_flex},
    component::{
        ActiveTheme as _, Icon, Sizable as _, Size,
        button::{Button, ButtonVariants as _},
        slider::{Slider, SliderState},
        spinner::Spinner,
    },
    div, img, linear_color_stop, linear_gradient,
    prelude::FluentBuilder as _,
    px, relative,
};

use crate::components::marquee::Marquee;

const BAR_HEIGHT: f32 = 72.;
const ARTWORK_SIZE: f32 = 48.;
const IDENTITY_WIDTH: f32 = 220.;
const EXTRAS_WIDTH: f32 = 260.;
const PLAY_BUTTON_SIZE: f32 = 32.;
const PROGRESS_WIDTH: f32 = 360.;
const VOLUME_WIDTH: f32 = 120.;
const VOLUME_TRACK_HEIGHT: f32 = 4.;
const VOLUME_THUMB_SIZE: f32 = 10.;
const SEEK_THUMB_SIZE: f32 = 8.;

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

#[derive(IntoElement)]
pub struct PlayerBar {
    id: SharedString,
    title: SharedString,
    artist: Option<SharedString>,
    thumbnail_url: Option<SharedString>,
    loading: bool,
    playing: bool,
    shuffle_active: bool,
    repeat_active: bool,
    position: SharedString,
    duration: SharedString,
    progress: f32,
    volume_slider: Option<Entity<SliderState>>,
    seek_slider: Option<Entity<SliderState>>,
    on_artist: Option<Box<dyn Fn(&mut App) + 'static>>,
    on_queue: Option<ClickHandler>,
    on_info: Option<ClickHandler>,
    on_play_pause: Option<ClickHandler>,
    on_previous: Option<ClickHandler>,
    on_next: Option<ClickHandler>,
    on_shuffle: Option<ClickHandler>,
    on_repeat: Option<ClickHandler>,
}

impl PlayerBar {
    pub fn new(id: impl Into<SharedString>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            artist: None,
            thumbnail_url: None,
            loading: false,
            playing: false,
            shuffle_active: false,
            repeat_active: false,
            position: SharedString::default(),
            duration: SharedString::default(),
            progress: 0.,
            volume_slider: None,
            seek_slider: None,
            on_artist: None,
            on_queue: None,
            on_info: None,
            on_play_pause: None,
            on_previous: None,
            on_next: None,
            on_shuffle: None,
            on_repeat: None,
        }
    }

    pub fn artist(mut self, artist: impl Into<SharedString>) -> Self {
        self.artist = Some(artist.into());
        self
    }

    pub fn thumbnail(mut self, url: impl Into<SharedString>) -> Self {
        self.thumbnail_url = Some(url.into());
        self
    }

    pub fn playing(mut self, playing: bool) -> Self {
        self.playing = playing;
        self
    }

    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    pub fn shuffle_active(mut self, active: bool) -> Self {
        self.shuffle_active = active;
        self
    }

    pub fn repeat_active(mut self, active: bool) -> Self {
        self.repeat_active = active;
        self
    }

    pub fn position(mut self, position: impl Into<SharedString>) -> Self {
        self.position = position.into();
        self
    }

    pub fn duration(mut self, duration: impl Into<SharedString>) -> Self {
        self.duration = duration.into();
        self
    }

    pub fn progress(mut self, progress: f32) -> Self {
        self.progress = progress.clamp(0., 1.);
        self
    }

    pub fn volume_slider(mut self, state: &Entity<SliderState>) -> Self {
        self.volume_slider = Some(state.clone());
        self
    }

    pub fn seek_slider(mut self, state: &Entity<SliderState>) -> Self {
        self.seek_slider = Some(state.clone());
        self
    }

    pub fn on_artist(mut self, handler: impl Fn(&mut App) + 'static) -> Self {
        self.on_artist = Some(Box::new(handler));
        self
    }

    pub fn on_queue(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_queue = Some(Box::new(handler));
        self
    }

    pub fn on_info(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_info = Some(Box::new(handler));
        self
    }

    pub fn on_play_pause(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_play_pause = Some(Box::new(handler));
        self
    }

    pub fn on_previous(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_previous = Some(Box::new(handler));
        self
    }

    pub fn on_next(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_next = Some(Box::new(handler));
        self
    }

    pub fn on_shuffle(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_shuffle = Some(Box::new(handler));
        self
    }

    pub fn on_repeat(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_repeat = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for PlayerBar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let PlayerBar {
            id,
            title,
            artist,
            thumbnail_url,
            loading,
            playing,
            shuffle_active,
            repeat_active,
            position,
            duration,
            progress,
            volume_slider,
            seek_slider,
            on_artist,
            on_queue,
            on_info,
            on_play_pause,
            on_previous,
            on_next,
            on_shuffle,
            on_repeat,
        } = self;

        let theme = cx.theme();
        let background = theme.status_bar;
        let border = theme.status_bar_border;
        let foreground = theme.foreground;
        let muted = theme.muted_foreground;
        let track_bg = theme.muted;
        let play_bg = theme.primary;
        let play_fg = theme.primary_foreground;
        let volume_low = theme.success;
        let volume_mid = theme.warning;
        let volume_high = theme.danger;

        let artwork = div()
            .size(px(ARTWORK_SIZE))
            .flex_shrink_0()
            .rounded(px(4.))
            .overflow_hidden()
            .flex()
            .items_center()
            .justify_center()
            .bg(track_bg)
            .map(|this| match thumbnail_url {
                Some(url) => {
                    this.child(img(url).size(px(ARTWORK_SIZE)).object_fit(ObjectFit::Cover))
                }
                None => this.child(Icon::new(IconName::Music).size_4().text_color(muted)),
            });

        let identity = h_flex()
            .flex_shrink_0()
            .w(px(IDENTITY_WIDTH))
            .min_w_0()
            .gap_2()
            .items_center()
            .child(artwork)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        h_flex().w_full().min_w_0().gap_1p5().items_center().child(
                            div().min_w_0().flex_1().child(Marquee::new(
                                format!("{id}-title"),
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(foreground)
                                    .child(title),
                            )),
                        ),
                    )
                    .when_some(artist, |this, artist| match on_artist {
                        Some(handler) => this.child(
                            h_flex().w_full().min_w_0().child(
                                div()
                                    .id(format!("{id}-artist-link"))
                                    .max_w_full()
                                    .text_color(muted)
                                    .cursor_pointer()
                                    .hover(|style| style.text_color(foreground).underline())
                                    .on_click(move |_, _, cx| handler(cx))
                                    .child(
                                        Marquee::new(
                                            format!("{id}-artist"),
                                            div().text_xs().child(artist),
                                        )
                                        .fit_content(),
                                    ),
                            ),
                        ),
                        None => this.child(Marquee::new(
                            format!("{id}-artist"),
                            div().text_xs().text_color(muted).child(artist),
                        )),
                    }),
            );

        let play_pause_icon = match playing {
            true => IconName::Pause,
            false => IconName::Play,
        };
        let play_pause = div()
            .id(format!("{id}-play-pause"))
            .size(px(PLAY_BUTTON_SIZE))
            .flex_shrink_0()
            .rounded(px(99.))
            .flex()
            .items_center()
            .justify_center()
            .bg(play_bg)
            .map(|this| match loading {
                true => this.child(Spinner::new().with_size(Size::Small).color(play_fg)),
                false => this.child(Icon::new(play_pause_icon).size_4().text_color(play_fg)),
            })
            .when(!loading, |this| this.cursor_pointer())
            .map(|this| match (loading, on_play_pause) {
                (false, Some(handler)) => {
                    this.on_click(move |event, window, cx| handler(event, window, cx))
                }
                _ => this,
            });

        let transport = h_flex()
            .items_center()
            .gap_4()
            .child(transport_button(
                &id,
                "shuffle",
                IconName::Shuffle,
                shuffle_active,
                foreground,
                muted,
                on_shuffle,
            ))
            .child(transport_button(
                &id,
                "previous",
                IconName::SkipBack,
                false,
                foreground,
                muted,
                on_previous,
            ))
            .child(play_pause)
            .child(transport_button(
                &id,
                "next",
                IconName::SkipForward,
                false,
                foreground,
                muted,
                on_next,
            ))
            .child(transport_button(
                &id,
                "repeat",
                IconName::Repeat,
                repeat_active,
                foreground,
                muted,
                on_repeat,
            ));

        let progress_bar = h_flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .flex_shrink_0()
                    .child(position),
            )
            .child(match seek_slider {
                Some(state) => {
                    seek_control(state, progress, foreground, track_bg).into_any_element()
                }
                None => filled_track(px(PROGRESS_WIDTH), progress, foreground, track_bg)
                    .into_any_element(),
            })
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .flex_shrink_0()
                    .child(duration),
            );

        let center = v_flex()
            .flex_1()
            .items_center()
            .gap_1()
            .child(transport)
            .child(progress_bar);

        let volume_fraction = volume_slider
            .as_ref()
            .map_or(0., |state| state.read(cx).percentage().end);
        let volume_icon = match volume_fraction {
            f if f <= 0. => IconName::VolumeX,
            f if f <= 0.5 => IconName::Volume1,
            _ => IconName::Volume2,
        };

        let extras = h_flex()
            .flex_shrink_0()
            .w(px(EXTRAS_WIDTH))
            .justify_end()
            .items_center()
            .gap_3()
            .child({
                let button = icon_button(&id, "queue", IconName::ListMusic, muted);
                match on_queue {
                    Some(handler) => {
                        button.on_click(move |event, window, cx| handler(event, window, cx))
                    }
                    None => button,
                }
            })
            .when_some(on_info, |this, handler| {
                this.child(
                    Button::new(format!("{id}-info"))
                        .cursor_pointer()
                        .ghost()
                        .icon(Icon::new(IconName::Info).size_4().text_color(muted))
                        .on_click(move |event, window, cx| handler(event, window, cx)),
                )
            })
            .child(Icon::new(volume_icon).size_4().text_color(muted))
            .when_some(volume_slider, |this, state| {
                this.child(volume_control(
                    state,
                    volume_fraction,
                    track_bg,
                    foreground,
                    volume_low,
                    volume_mid,
                    volume_high,
                ))
            });

        h_flex()
            .id(id)
            .w_full()
            .h(px(BAR_HEIGHT))
            .px_4()
            .items_center()
            .justify_between()
            .bg(background)
            .border_t_1()
            .border_color(border)
            .child(identity)
            .child(center)
            .child(extras)
    }
}

fn transport_button(
    id: &str,
    suffix: &'static str,
    icon: IconName,
    active: bool,
    foreground: Hsla,
    muted: Hsla,
    handler: Option<ClickHandler>,
) -> Button {
    let color = match active {
        true => foreground,
        false => muted,
    };
    let button = Button::new(format!("{id}-{suffix}"))
        .cursor_pointer()
        .ghost()
        .icon(Icon::new(icon).size_4().text_color(color));
    match handler {
        Some(handler) => button.on_click(move |event, window, cx| handler(event, window, cx)),
        None => button,
    }
}

fn icon_button(id: &str, suffix: &'static str, icon: IconName, color: Hsla) -> Button {
    Button::new(format!("{id}-{suffix}"))
        .cursor_pointer()
        .ghost()
        .icon(Icon::new(icon).size_4().text_color(color))
}

const VOLUME_HIT_HEIGHT: f32 = 24.;

fn volume_control(
    state: Entity<SliderState>,
    fraction: f32,
    track: Hsla,
    thumb: Hsla,
    low: Hsla,
    mid: Hsla,
    high: Hsla,
) -> impl IntoElement {
    let half = px(VOLUME_WIDTH / 2.);

    let full_gradient = h_flex()
        .absolute()
        .inset_0()
        .child(div().w(half).h_full().flex_shrink_0().bg(linear_gradient(
            90.,
            linear_color_stop(low, 0.),
            linear_color_stop(mid, 1.),
        )))
        .child(div().w(half).h_full().flex_shrink_0().bg(linear_gradient(
            90.,
            linear_color_stop(mid, 0.),
            linear_color_stop(high, 1.),
        )));

    let gradient_track = div()
        .absolute()
        .left_0()
        .right_0()
        .top(relative(0.5))
        .mt(-px(VOLUME_TRACK_HEIGHT / 2.))
        .h(px(VOLUME_TRACK_HEIGHT))
        .rounded(px(99.))
        .overflow_hidden()
        .bg(track)
        .child(
            div()
                .absolute()
                .left_0()
                .top_0()
                .bottom_0()
                .overflow_hidden()
                .w(relative(fraction))
                .child(full_gradient),
        );

    let hit_area = Slider::new(&state)
        .cursor_pointer()
        .absolute()
        .inset_0()
        .bg(Hsla::transparent_black())
        .text_color(Hsla::transparent_black());

    let thumb_dot = div()
        .absolute()
        .left(relative(fraction))
        .top(relative(0.5))
        .ml(-px(VOLUME_THUMB_SIZE / 2.))
        .mt(-px(VOLUME_THUMB_SIZE / 2.))
        .size(px(VOLUME_THUMB_SIZE))
        .rounded(px(99.))
        .bg(thumb);

    div()
        .relative()
        .w(px(VOLUME_WIDTH))
        .h(px(VOLUME_HIT_HEIGHT))
        .child(gradient_track)
        .child(hit_area)
        .child(thumb_dot)
}

fn seek_control(
    state: Entity<SliderState>,
    fraction: f32,
    fill: Hsla,
    track: Hsla,
) -> impl IntoElement {
    let hit_area = div().absolute().inset_0().opacity(0.).child(
        Slider::new(&state)
            .cursor_pointer()
            .absolute()
            .inset_0()
            .bg(Hsla::transparent_black())
            .text_color(Hsla::transparent_black()),
    );

    let thumb_dot = div()
        .absolute()
        .left(relative(fraction))
        .top(relative(0.5))
        .ml(-px(SEEK_THUMB_SIZE / 2.))
        .mt(-px(SEEK_THUMB_SIZE / 2.))
        .size(px(SEEK_THUMB_SIZE))
        .rounded(px(99.))
        .bg(fill)
        .invisible()
        .group_hover("seek-control", |style| style.visible());

    div()
        .group("seek-control")
        .relative()
        .w(px(PROGRESS_WIDTH))
        .h(px(VOLUME_HIT_HEIGHT))
        .flex_shrink_0()
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(relative(0.5))
                .mt(-px(2.))
                .child(filled_track(px(PROGRESS_WIDTH), fraction, fill, track)),
        )
        .child(hit_area)
        .child(thumb_dot)
}

fn filled_track(width: Pixels, fraction: f32, fill: Hsla, track: Hsla) -> impl IntoElement {
    div()
        .w(width)
        .h(px(4.))
        .flex_shrink_0()
        .rounded(px(99.))
        .bg(track)
        .child(
            div()
                .h_full()
                .rounded(px(99.))
                .bg(fill)
                .w(relative(fraction)),
        )
}
