use std::time::Duration;

use encore_auth::{has_session, same_jar};

pub const CHECK_INTERVAL: Duration = Duration::from_secs(30);
pub const SETTLE_AFTER_LOAD: Duration = Duration::from_secs(5);
pub const RELOAD_INTERVAL: Duration = Duration::from_secs(30 * 60);
const MISSES_BEFORE_SIGNED_OUT: u32 = 3;

#[derive(Debug, PartialEq, Eq)]
pub enum Check {
    Nothing,
    Emit(String),
    SignedOut,
}

pub struct KeepAlive {
    last: String,
    misses: u32,
}

impl KeepAlive {
    pub fn new(current: String) -> Self {
        Self {
            last: current,
            misses: 0,
        }
    }

    pub fn check(&mut self, header: String) -> Check {
        if !has_session(&header) {
            self.misses += 1;
            return match self.misses >= MISSES_BEFORE_SIGNED_OUT {
                true => Check::SignedOut,
                false => Check::Nothing,
            };
        }

        self.misses = 0;
        if same_jar(&header, &self.last) {
            return Check::Nothing;
        }
        self.last = header.clone();
        Check::Emit(header)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_jar_the_app_already_has_is_not_emitted() {
        let mut keep_alive = KeepAlive::new("SAPISID=a; TS=old".to_owned());

        assert_eq!(
            keep_alive.check("TS=old; SAPISID=a".to_owned()),
            Check::Nothing
        );
    }

    #[test]
    fn a_rotated_jar_is_emitted_once() {
        let mut keep_alive = KeepAlive::new("SAPISID=a; TS=old".to_owned());

        assert_eq!(
            keep_alive.check("SAPISID=a; TS=new".to_owned()),
            Check::Emit("SAPISID=a; TS=new".to_owned())
        );
        assert_eq!(
            keep_alive.check("SAPISID=a; TS=new".to_owned()),
            Check::Nothing
        );
    }

    #[test]
    fn a_few_empty_reads_are_tolerated_but_a_streak_means_signed_out() {
        let mut keep_alive = KeepAlive::new("SAPISID=a".to_owned());

        assert_eq!(keep_alive.check(String::new()), Check::Nothing);
        assert_eq!(keep_alive.check(String::new()), Check::Nothing);
        assert_eq!(keep_alive.check(String::new()), Check::SignedOut);
    }

    #[test]
    fn a_session_in_between_resets_the_streak() {
        let mut keep_alive = KeepAlive::new("SAPISID=a".to_owned());

        keep_alive.check(String::new());
        keep_alive.check(String::new());
        keep_alive.check("SAPISID=a".to_owned());

        assert_eq!(keep_alive.check(String::new()), Check::Nothing);
    }
}
