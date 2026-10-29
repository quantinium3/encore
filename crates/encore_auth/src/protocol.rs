pub const LOGIN_BINARY: &str = "encore-login";

pub const FLAG_DATA_DIR: &str = "--data-dir";

pub const COMMAND_LOGIN: &str = "login";
pub const COMMAND_REFRESH: &str = "refresh";
pub const COMMAND_KEEP_ALIVE: &str = "keep-alive";

pub const EXIT_CANCELLED: i32 = 10;
pub const EXIT_NOT_SIGNED_IN: i32 = 11;
pub const EXIT_UNCHANGED: i32 = 12;
pub const EXIT_TIMED_OUT: i32 = 13;

#[cfg(test)]
mod tests {
    use super::*;

    const CLAP_USAGE_ERROR: i32 = 2;

    #[test]
    fn outcome_codes_never_collide_with_generic_failures() {
        let outcomes = [
            EXIT_CANCELLED,
            EXIT_NOT_SIGNED_IN,
            EXIT_UNCHANGED,
            EXIT_TIMED_OUT,
        ];

        for code in outcomes {
            assert!(code > CLAP_USAGE_ERROR, "{code}");
            assert_ne!(code, 101, "{code}");
        }
        for (index, code) in outcomes.iter().enumerate() {
            assert!(!outcomes[index + 1..].contains(code), "{code}");
        }
    }
}
