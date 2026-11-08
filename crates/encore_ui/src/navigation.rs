use gpui_kit::SharedString;

use crate::search::SearchKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Location {
    Playlist(Option<SharedString>),
    Artist(SharedString),
    Search { query: SharedString, kind: SearchKind },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct History<T> {
    entries: Vec<T>,
    cursor: usize,
}

impl<T> Default for History<T> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            cursor: 0,
        }
    }
}

impl<T: PartialEq + Clone> History<T> {
    pub fn current(&self) -> Option<&T> {
        self.entries.get(self.cursor)
    }

    pub fn can_go_back(&self) -> bool {
        self.cursor > 0
    }

    pub fn can_go_forward(&self) -> bool {
        self.cursor + 1 < self.entries.len()
    }

    pub fn record(&mut self, location: T) {
        if self.current() == Some(&location) {
            return;
        }
        if self.entries.is_empty() {
            self.entries.push(location);
            return;
        }
        self.entries.truncate(self.cursor + 1);
        self.entries.push(location);
        self.cursor += 1;
    }

    pub fn replace_current(&mut self, location: T) {
        match self.entries.get_mut(self.cursor) {
            Some(entry) => *entry = location,
            None => self.entries.push(location),
        }
    }

    pub fn go_back(&mut self) -> Option<T> {
        if !self.can_go_back() {
            return None;
        }
        self.cursor -= 1;
        self.current().cloned()
    }

    pub fn go_forward(&mut self) -> Option<T> {
        if !self.can_go_forward() {
            return None;
        }
        self.cursor += 1;
        self.current().cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history(visits: &[&'static str]) -> History<&'static str> {
        let mut history = History::default();
        for visit in visits {
            history.record(*visit);
        }
        history
    }

    #[test]
    fn going_back_and_forward_walks_the_visits() {
        let mut history = history(&["a", "b", "c"]);
        assert_eq!(history.go_back(), Some("b"));
        assert_eq!(history.go_back(), Some("a"));
        assert_eq!(history.go_back(), None);
        assert_eq!(history.go_forward(), Some("b"));
    }

    #[test]
    fn visiting_somewhere_new_drops_the_forward_history() {
        let mut history = history(&["a", "b", "c"]);
        history.go_back();
        history.go_back();
        history.record("d");
        assert!(!history.can_go_forward());
        assert_eq!(history.go_back(), Some("a"));
    }
}
