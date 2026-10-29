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
