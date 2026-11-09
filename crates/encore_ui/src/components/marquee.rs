use std::time::{Duration, Instant};

use gpui_kit::{
    AnyElement, App, AvailableSpace, Bounds, ContentMask, Element, ElementId, Entity,
    GlobalElementId, InspectorElementId, InteractiveElement as _, IntoElement, LayoutId,
    ParentElement as _, Pixels, Point, RenderOnce, Size, StatefulInteractiveElement as _, Style,
    Styled as _, Window, div, prelude::FluentBuilder as _, px, relative, size,
};

const SCROLL_SPEED: f32 = 36.;
const HOLD: Duration = Duration::from_millis(550);

#[derive(IntoElement)]
pub struct Marquee {
    id: ElementId,
    child: AnyElement,
    fit_content: bool,
}

impl Marquee {
    #[track_caller]
    pub fn new(id: impl Into<ElementId>, child: impl IntoElement) -> Self {
        Self {
            id: id.into(),
            child: child.into_any_element(),
            fit_content: false,
        }
    }

    pub fn fit_content(mut self) -> Self {
        self.fit_content = true;
        self
    }
}

#[derive(Clone, Copy, Default)]
struct MarqueeHoverState {
    hovered: bool,
    content_width: Option<Pixels>,
}

impl RenderOnce for Marquee {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Marquee {
            id,
            child,
            fit_content,
        } = self;

        let hover_state =
            window.use_keyed_state(id.clone(), cx, |_, _| MarqueeHoverState::default());
        let hovered = hover_state.read(cx).hovered;
        let content_width = hover_state.read(cx).content_width;

        div()
            .id(id.clone())
            .min_w_0()
            .map(|this| match (fit_content, content_width) {
                (true, Some(width)) => this.w(width).max_w_full(),
                (true, None) => this.max_w_full(),
                (false, _) => this.w_full(),
            })
            .on_hover(
                window.listener_for(&hover_state, move |state, hovered, _, cx| {
                    if state.hovered != *hovered {
                        state.hovered = *hovered;
                        cx.notify();
                    }
                }),
            )
            .child(MarqueeTrack {
                id: (id, "track").into(),
                hovered,
                child,
                report_width: fit_content.then_some(hover_state),
            })
    }
}

struct MarqueeTrack {
    id: ElementId,
    hovered: bool,
    child: AnyElement,
    report_width: Option<Entity<MarqueeHoverState>>,
}

#[derive(Clone, Copy, Default)]
struct MarqueeTrackState {
    content_size: Option<Size<Pixels>>,
    started_at: Option<Instant>,
}

impl IntoElement for MarqueeTrack {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for MarqueeTrack {
    type RequestLayoutState = Size<Pixels>;
    type PrepaintState = Pixels;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        global_id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let measured = self.child.layout_as_root(
            size(AvailableSpace::MaxContent, AvailableSpace::MaxContent),
            window,
            cx,
        );

        let global_id = global_id.expect("MarqueeTrack must have an id");
        let cached_height =
            window.with_element_state(global_id, |state: Option<MarqueeTrackState>, _| {
                let mut state = state.unwrap_or_default();
                state.content_size = Some(measured);
                (measured.height, state)
            });

        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = cached_height.into();

        (window.request_layout(style, None, cx), measured)
    }

    fn prepaint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let measured = self.child.layout_as_root(
            size(AvailableSpace::MaxContent, AvailableSpace::MaxContent),
            window,
            cx,
        );
        if let Some(state) = &self.report_width
            && state.read(cx).content_width != Some(measured.width)
        {
            state.update(cx, |state, cx| {
                state.content_width = Some(measured.width);
                cx.notify();
            });
        }
        let overflow = (measured.width - bounds.size.width).max(px(0.));
        let hovered = self.hovered;

        let offset = window.with_element_state(
            global_id.expect("MarqueeTrack must have an id"),
            |state: Option<MarqueeTrackState>, window| {
                let mut state = state.unwrap_or_default();
                let size_changed = state.content_size != Some(measured);
                state.content_size = Some(measured);

                if !hovered || overflow <= px(0.) {
                    state.started_at = None;
                    if size_changed {
                        window.request_animation_frame();
                    }
                    return (px(0.), state);
                }

                let started_at = *state.started_at.get_or_insert_with(Instant::now);
                let elapsed = started_at.elapsed().as_secs_f32();
                let overflow_f = f32::from(overflow);

                let scroll_secs = (overflow_f / SCROLL_SPEED).max(0.05);
                let hold_secs = HOLD.as_secs_f32();
                let cycle_secs = hold_secs * 2. + scroll_secs * 2.;
                let t = elapsed % cycle_secs;

                let offset = if t < hold_secs {
                    0.
                } else if t < hold_secs + scroll_secs {
                    -overflow_f * (t - hold_secs) / scroll_secs
                } else if t < hold_secs * 2. + scroll_secs {
                    -overflow_f
                } else {
                    let back = (t - hold_secs * 2. - scroll_secs) / scroll_secs;
                    -overflow_f * (1. - back)
                };

                window.request_animation_frame();
                (px(offset), state)
            },
        );

        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            self.child.prepaint_at(
                Point {
                    x: bounds.origin.x + offset,
                    y: bounds.origin.y,
                },
                window,
                cx,
            );
        });

        offset
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            self.child.paint(window, cx);
        });
    }
}
