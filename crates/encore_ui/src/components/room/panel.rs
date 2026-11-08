use gpui_kit::{
    AppContext as _, ClipboardItem, Context, Entity, FontWeight, IntoElement, ParentElement as _, Render,
    SharedString, Styled as _, Subscription, Window,
    base::{h_flex, v_flex},
    component::{
        ActiveTheme as _, Disableable as _, Sizable as _, WindowExt as _,
        button::Button,
        input::{Input, InputEvent, InputState},
        notification::Notification,
    },
    div,
    prelude::FluentBuilder as _,
    px,
};

use crate::room::{RoomModel, RoomStatus};

const PANEL_WIDTH: f32 = 300.;

pub struct RoomPanel {
    model: Entity<RoomModel>,
    code_input: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

impl RoomPanel {
    pub fn new(model: Entity<RoomModel>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let code_input = cx.new(|cx| InputState::new(window, cx).placeholder("Room code"));
        let subscriptions = vec![
            cx.observe(&model, |_, _, cx| cx.notify()),
            cx.subscribe(&code_input, |this, _, event: &InputEvent, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.join(cx);
                }
            }),
        ];
        Self {
            model,
            code_input,
            _subscriptions: subscriptions,
        }
    }

    fn join(&mut self, cx: &mut Context<Self>) {
        let code = self.code_input.read(cx).value();
        if code.trim().is_empty() {
            return;
        }
        self.model.update(cx, |model, cx| model.request_join(code, cx));
    }
}

impl Render for RoomPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let model = self.model.read(cx);
        let status = model.status().clone();
        let error = model.error().cloned();

        let body = match status {
            RoomStatus::Connected { code, members } => {
                let copied = code.clone();
                v_flex()
                    .gap_3()
                    .child(
                        v_flex()
                            .gap_1()
                            .child(div().text_xs().text_color(muted).child("Room code"))
                            .child(
                                div()
                                    .text_2xl()
                                    .font_weight(FontWeight::BOLD)
                                    .child(code),
                            )
                            .child(div().text_xs().text_color(muted).child(match members {
                                1 => "1 listener".to_owned(),
                                count => format!("{count} listeners"),
                            })),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("room-copy")
                                    .cursor_pointer()
                                    .outline()
                                    .small()
                                    .label("Copy code")
                                    .on_click(move |_, window, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            copied.to_string(),
                                        ));
                                        window.push_notification(
                                            Notification::success("Room code copied"),
                                            cx,
                                        );
                                    }),
                            )
                            .child(
                                Button::new("room-leave")
                                    .cursor_pointer()
                                    .outline()
                                    .small()
                                    .label("Leave")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.model.update(cx, |model, cx| model.request_leave(cx));
                                    })),
                            ),
                    )
            }
            status => {
                let joining = status == RoomStatus::Joining;
                v_flex()
                    .gap_3()
                    .child(
                        Button::new("room-create")
                            .cursor_pointer()
                            .small()
                            .label("Start a room")
                            .loading(joining)
                            .disabled(joining)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.model.update(cx, |model, cx| model.request_create(cx));
                            })),
                    )
                    .child(div().text_xs().text_color(muted).child("or join with a code"))
                    .child(
                        h_flex()
                            .gap_2()
                            .child(div().flex_1().child(Input::new(&self.code_input)))
                            .child(
                                Button::new("room-join")
                                    .cursor_pointer()
                                    .outline()
                                    .small()
                                    .label("Join")
                                    .disabled(joining)
                                    .on_click(cx.listener(|this, _, _, cx| this.join(cx))),
                            ),
                    )
            }
        };

        v_flex()
            .w(px(PANEL_WIDTH))
            .gap_3()
            .p_4()
            .rounded(theme.radius_lg)
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .shadow_lg()
            .child(div().font_weight(FontWeight::SEMIBOLD).child("Listen together"))
            .child(body)
            .when_some(error, |this, error: SharedString| {
                this.child(div().text_xs().text_color(theme.danger).child(error))
            })
    }
}
