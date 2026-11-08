use gpui_kit::{Context, EventEmitter, SharedString};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RoomStatus {
    #[default]
    Idle,
    Joining,
    Connected {
        code: SharedString,
        members: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoomEvent {
    CreateRequested,
    JoinRequested(SharedString),
    LeaveRequested,
}

#[derive(Default)]
pub struct RoomModel {
    status: RoomStatus,
    error: Option<SharedString>,
    open: bool,
}

impl EventEmitter<RoomEvent> for RoomModel {}

impl RoomModel {
    pub fn status(&self) -> &RoomStatus {
        &self.status
    }

    pub fn error(&self) -> Option<&SharedString> {
        self.error.as_ref()
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn is_connected(&self) -> bool {
        matches!(self.status, RoomStatus::Connected { .. })
    }

    pub fn toggle_open(&mut self, cx: &mut Context<Self>) {
        self.open = !self.open;
        cx.notify();
    }

    pub fn request_create(&mut self, cx: &mut Context<Self>) {
        if self.status == RoomStatus::Idle {
            self.error = None;
            cx.emit(RoomEvent::CreateRequested);
        }
    }

    pub fn request_join(&mut self, code: SharedString, cx: &mut Context<Self>) {
        if self.status == RoomStatus::Idle {
            self.error = None;
            cx.emit(RoomEvent::JoinRequested(code));
        }
    }

    pub fn request_leave(&mut self, cx: &mut Context<Self>) {
        if self.status != RoomStatus::Idle {
            cx.emit(RoomEvent::LeaveRequested);
        }
    }

    pub fn set_joining(&mut self, cx: &mut Context<Self>) {
        self.status = RoomStatus::Joining;
        self.error = None;
        cx.notify();
    }

    pub fn set_connected(&mut self, code: SharedString, members: usize, cx: &mut Context<Self>) {
        let next = RoomStatus::Connected { code, members };
        if self.status != next {
            self.status = next;
            self.error = None;
            cx.notify();
        }
    }

    pub fn set_idle(&mut self, error: Option<SharedString>, cx: &mut Context<Self>) {
        self.status = RoomStatus::Idle;
        self.error = error;
        cx.notify();
    }
}
