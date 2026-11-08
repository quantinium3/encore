use gpui_kit::{
    Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window,
    assets::IconName,
    base::{h_flex, v_flex},
    component::{
        ActiveTheme as _, Icon, Sizable as _,
        button::{Button, ButtonVariants as _},
    },
    div, px,
};

use crate::components::playlist::detail::render_unnumbered_track;
use crate::player::PlayerModel;

pub const QUEUE_PANEL_WIDTH: f32 = 340.;

#[derive(Default)]
pub struct QueuePanelState {
    open: bool,
}

impl QueuePanelState {
    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        self.open = !self.open;
        cx.notify();
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        if self.open {
            self.open = false;
            cx.notify();
        }
    }
}

pub struct QueuePanel {
    model: Entity<PlayerModel>,
    state: Entity<QueuePanelState>,
    _observe: Subscription,
}

impl QueuePanel {
    pub fn new(
        model: Entity<PlayerModel>,
        state: Entity<QueuePanelState>,
        cx: &mut Context<Self>,
    ) -> Self {
        let observe = cx.observe(&model, |_, _, cx| cx.notify());
        Self {
            model,
            state,
            _observe: observe,
        }
    }
}

impl Render for QueuePanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (border, muted) = (theme.border, theme.muted_foreground);
        let model = self.model.clone();
        let (queue, current, status) = {
            let state = model.read(cx);
            (
                state.queue().to_vec(),
                state.current_index(),
                state.status(),
            )
        };
        let state = self.state.clone();

        let body = match queue.is_empty() {
            true => div()
                .p_4()
                .text_sm()
                .text_color(muted)
                .child("The queue is empty.")
                .into_any_element(),
            false => v_flex()
                .w_full()
                .min_w_0()
                .p_2()
                .gap_1()
                .children(queue.iter().cloned().enumerate().map(|(index, track)| {
                    let is_current = current == Some(index);
                    render_unnumbered_track(
                        index,
                        track,
                        is_current,
                        is_current && status.is_loading(),
                        is_current && status.is_playing(),
                        queue.clone(),
                        &model,
                    )
                }))
                .into_any_element(),
        };

        v_flex()
            .w(px(QUEUE_PANEL_WIDTH))
            .h_full()
            .flex_shrink_0()
            .border_l_1()
            .border_color(border)
            .child(
                h_flex()
                    .w_full()
                    .px_4()
                    .h(px(48.))
                    .flex_shrink_0()
                    .items_center()
                    .justify_between()
                    .child(div().font_weight(gpui_kit::FontWeight::SEMIBOLD).child("Queue"))
                    .child(
                        Button::new("queue-close")
                            .cursor_pointer()
                            .ghost()
                            .xsmall()
                            .icon(Icon::new(IconName::X).size_4())
                            .on_click(move |_, _, cx| {
                                state.update(cx, |state, cx| state.close(cx));
                            }),
                    ),
            )
            .child(
                div()
                    .id("queue-panel-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(body),
            )
    }
}
