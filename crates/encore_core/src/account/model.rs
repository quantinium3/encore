use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AccountId(String);

impl AccountId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for AccountId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

impl fmt::Display for AccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AccountProfile {
    pub name: String,
    pub detail: Option<String>,
    pub avatar_url: Option<String>,
}

impl AccountProfile {
    pub fn merged_with(self, newer: AccountProfile) -> AccountProfile {
        AccountProfile {
            name: match newer.name.is_empty() {
                true => self.name,
                false => newer.name,
            },
            detail: newer.detail.or(self.detail),
            avatar_url: newer.avatar_url.or(self.avatar_url),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Account {
    pub id: AccountId,
    pub session_cookie: String,
    pub added_at_unix: i64,
    pub profile: AccountProfile,
}

impl fmt::Debug for Account {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Account")
            .field("id", &self.id)
            .field("session_cookie", &"<redacted>")
            .field("added_at_unix", &self.added_at_unix)
            .field("profile", &self.profile)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(name: &str, detail: Option<&str>, avatar: Option<&str>) -> AccountProfile {
        AccountProfile {
            name: name.to_owned(),
            detail: detail.map(str::to_owned),
            avatar_url: avatar.map(str::to_owned),
        }
    }

    #[test]
    fn a_newer_profile_replaces_the_fields_it_has() {
        let old = profile("Old", Some("@old"), Some("old.jpg"));
        let new = profile("New", Some("@new"), Some("new.jpg"));

        assert_eq!(old.merged_with(new.clone()), new);
    }

    #[test]
    fn a_newer_profile_never_blanks_what_it_does_not_know() {
        let old = profile("Old", Some("@old"), Some("old.jpg"));

        let merged = old.clone().merged_with(AccountProfile::default());

        assert_eq!(merged, old);
    }

    #[test]
    fn fields_merge_independently() {
        let old = profile("Old", Some("@old"), None);
        let new = profile("", None, Some("new.jpg"));

        assert_eq!(
            old.merged_with(new),
            profile("Old", Some("@old"), Some("new.jpg"))
        );
    }

    #[test]
    fn the_session_cookie_never_shows_in_debug_output() {
        let account = Account {
            id: AccountId::new("a"),
            session_cookie: "supersecret".into(),
            added_at_unix: 1,
            profile: AccountProfile::default(),
        };

        assert!(!format!("{account:?}").contains("supersecret"));
    }
}
