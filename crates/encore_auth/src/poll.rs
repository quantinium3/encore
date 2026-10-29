use std::time::Duration;

use encore_auth::protocol::{EXIT_NOT_SIGNED_IN, EXIT_UNCHANGED};
use encore_auth::{has_session, same_jar};

pub const POLL_INTERVAL: Duration = Duration::from_millis(500);
const LOGIN_ATTEMPTS: u32 = 6;
const REFRESH_ATTEMPTS: u32 = 8;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Judgement {
    NoSession,
    Unchanged,
    Fresh,
}

pub fn judge(header: &str, current: Option<&str>) -> Judgement {
    match (has_session(header), current) {
        (false, _) => Judgement::NoSession,
        (true, Some(current)) if same_jar(header, current) => Judgement::Unchanged,
        (true, _) => Judgement::Fresh,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    Retry,
    GiveUp,
    Emit(String),
    Exit(i32),
}

pub struct Poller {
    current: Option<String>,
    attempts: u32,
    remaining: u32,
    last: Judgement,
}

impl Poller {
    pub fn login() -> Self {
        Self::new(None, LOGIN_ATTEMPTS)
    }

    pub fn refresh(current: String) -> Self {
        Self::new(Some(current), REFRESH_ATTEMPTS)
    }

    fn new(current: Option<String>, attempts: u32) -> Self {
        Self {
            current,
            attempts,
            remaining: 0,
            last: Judgement::NoSession,
        }
    }

    pub fn is_active(&self) -> bool {
        self.remaining > 0
    }

    pub fn start(&mut self) -> bool {
        if self.is_active() {
            return false;
        }
        self.remaining = self.attempts;
        self.last = Judgement::NoSession;
        true
    }

    pub fn tick(&mut self, header: String) -> Step {
        let judgement = judge(&header, self.current.as_deref());
        if judgement == Judgement::Fresh {
            self.remaining = 0;
            return Step::Emit(header);
        }

        self.last = judgement;
        self.remaining -= 1;
        match (self.remaining, self.current.is_some()) {
            (0, true) => Step::Exit(exit_code(self.last)),
            (0, false) => Step::GiveUp,
            _ => Step::Retry,
        }
    }
}

fn exit_code(judgement: Judgement) -> i32 {
    match judgement {
        Judgement::Unchanged => EXIT_UNCHANGED,
        Judgement::Fresh | Judgement::NoSession => EXIT_NOT_SIGNED_IN,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_auth_cookie_means_not_signed_in() {
        assert_eq!(
            judge("VISITOR_INFO1_LIVE=x", Some("SAPISID=a")),
            Judgement::NoSession
        );
        assert_eq!(judge("", Some("SAPISID=a")), Judgement::NoSession);
    }

    #[test]
    fn identical_jar_in_any_order_is_unchanged() {
        assert_eq!(
            judge("SID=b; SAPISID=a", Some("SAPISID=a; SID=b")),
            Judgement::Unchanged
        );
    }

    #[test]
    fn a_rotated_jar_is_fresh() {
        assert_eq!(
            judge(
                "SAPISID=a; __Secure-3PSIDTS=new",
                Some("SAPISID=a; __Secure-3PSIDTS=old")
            ),
            Judgement::Fresh
        );
    }

    #[test]
    fn without_a_current_jar_any_session_is_fresh() {
        assert_eq!(judge("SAPISID=a", None), Judgement::Fresh);
        assert_eq!(judge("SID=1", None), Judgement::NoSession);
    }

    #[test]
    fn login_emits_the_first_header_with_a_session() {
        let mut poller = Poller::login();
        assert!(poller.start());

        assert_eq!(poller.tick("SID=1".to_owned()), Step::Retry);
        assert_eq!(
            poller.tick("SAPISID=a".to_owned()),
            Step::Emit("SAPISID=a".to_owned())
        );
        assert!(!poller.is_active());
    }

    #[test]
    fn login_gives_up_quietly_and_can_start_again() {
        let mut poller = Poller::login();
        poller.start();

        for _ in 1..LOGIN_ATTEMPTS {
            assert_eq!(poller.tick(String::new()), Step::Retry);
        }
        assert_eq!(poller.tick(String::new()), Step::GiveUp);
        assert!(!poller.is_active());
        assert!(poller.start());
    }

    #[test]
    fn start_does_not_restart_a_running_poll() {
        let mut poller = Poller::login();
        poller.start();
        poller.tick(String::new());

        assert!(!poller.start());
    }

    #[test]
    fn refresh_emits_as_soon_as_the_jar_rotates() {
        let mut poller = Poller::refresh("SAPISID=a; TS=old".to_owned());
        poller.start();

        assert_eq!(poller.tick("SAPISID=a; TS=old".to_owned()), Step::Retry);
        assert_eq!(
            poller.tick("SAPISID=a; TS=new".to_owned()),
            Step::Emit("SAPISID=a; TS=new".to_owned())
        );
    }

    #[test]
    fn refresh_that_never_rotates_exits_unchanged() {
        let mut poller = Poller::refresh("SAPISID=a".to_owned());
        poller.start();

        for _ in 1..REFRESH_ATTEMPTS {
            assert_eq!(poller.tick("SAPISID=a".to_owned()), Step::Retry);
        }
        assert_eq!(
            poller.tick("SAPISID=a".to_owned()),
            Step::Exit(EXIT_UNCHANGED)
        );
    }

    #[test]
    fn refresh_without_a_session_exits_not_signed_in() {
        let mut poller = Poller::refresh("SAPISID=a".to_owned());
        poller.start();

        for _ in 1..REFRESH_ATTEMPTS {
            poller.tick(String::new());
        }
        assert_eq!(poller.tick(String::new()), Step::Exit(EXIT_NOT_SIGNED_IN));
    }
}
