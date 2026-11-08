use std::fmt;

#[derive(Clone, PartialEq, Eq)]
pub struct Session {
    cookie: String,
}

impl Session {
    pub fn new(cookie: impl Into<String>) -> Self {
        Self {
            cookie: cookie.into(),
        }
    }

    pub fn cookie(&self) -> &str {
        &self.cookie
    }

    pub fn into_cookie(self) -> String {
        self.cookie
    }
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("cookie", &"<redacted>")
            .finish()
    }
}

const AUTH_COOKIES: [&str; 2] = ["SAPISID", "__Secure-3PAPISID"];

pub fn sapisid(cookie_header: &str) -> Option<&str> {
    AUTH_COOKIES.iter().find_map(|wanted| {
        cookie_header.split(';').find_map(|pair| {
            let (name, value) = pair.split_once('=')?;
            let value = value.trim();
            (name.trim() == *wanted && !value.is_empty()).then_some(value)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sapisid_cookie_is_found_wherever_it_sits() {
        assert_eq!(sapisid("SID=1; SAPISID=abc; PREF=x"), Some("abc"));
        assert_eq!(sapisid("SAPISID=abc"), Some("abc"));
    }

    #[test]
    fn the_secure_alias_is_used_when_sapisid_is_absent_or_empty() {
        assert_eq!(sapisid("__Secure-3PAPISID=alias"), Some("alias"));
        assert_eq!(sapisid("SAPISID=; __Secure-3PAPISID=alias"), Some("alias"));
    }

    #[test]
    fn sapisid_wins_over_the_alias_whatever_the_order() {
        assert_eq!(sapisid("__Secure-3PAPISID=b; SAPISID=a"), Some("a"));
    }

    #[test]
    fn no_auth_cookie_or_an_empty_one_is_no_sapisid() {
        assert_eq!(sapisid(""), None);
        assert_eq!(sapisid("SID=1; VISITOR_INFO1_LIVE=x"), None);
        assert_eq!(sapisid("SAPISID=  "), None);
        assert_eq!(sapisid("NOTSAPISID=abc"), None);
    }
}
