use super::RepeatMode;
use crate::Track;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybackQueue {
    tracks: Vec<Track>,
    current: Option<usize>,
    shuffle: bool,
    repeat: RepeatMode,
    shuffle_seed: u64,
}

impl PlaybackQueue {
    pub fn new(tracks: Vec<Track>) -> Self {
        Self {
            tracks,
            current: None,
            shuffle: false,
            repeat: RepeatMode::Off,
            shuffle_seed: 0x2545_F491_4F6C_DD1D,
        }
    }

    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    pub fn current_index(&self) -> Option<usize> {
        self.current
    }

    pub fn current_track(&self) -> Option<&Track> {
        self.current.and_then(|index| self.tracks.get(index))
    }

    pub fn shuffle(&self) -> bool {
        self.shuffle
    }

    pub fn repeat(&self) -> RepeatMode {
        self.repeat
    }

    pub fn set_shuffle(&mut self, shuffle: bool) {
        self.shuffle = shuffle;
    }

    pub fn set_repeat(&mut self, repeat: RepeatMode) {
        self.repeat = repeat;
    }

    pub fn append(&mut self, tracks: Vec<Track>) -> Vec<Track> {
        let fresh: Vec<Track> = tracks
            .into_iter()
            .filter(|track| self.tracks.iter().all(|queued| queued.id != track.id))
            .collect();
        self.tracks.extend(fresh.iter().cloned());
        fresh
    }

    pub fn peek_next(&self) -> Option<usize> {
        if self.shuffle || self.repeat == RepeatMode::One || self.tracks.is_empty() {
            return None;
        }
        let current = self.current?;
        let last = self.tracks.len() - 1;
        match (current < last, self.repeat) {
            (true, _) => Some(current + 1),
            (false, RepeatMode::All) => Some(0),
            (false, _) => None,
        }
    }

    pub fn remaining(&self) -> usize {
        match self.current {
            Some(current) => self.tracks.len().saturating_sub(current + 1),
            None => 0,
        }
    }

    pub fn play_at(&mut self, index: usize) -> Option<&Track> {
        if index >= self.tracks.len() {
            return None;
        }
        self.current = Some(index);
        self.tracks.get(index)
    }

    pub fn skip_forward(&mut self) -> Option<&Track> {
        let next = self.next_index()?;
        self.current = Some(next);
        self.tracks.get(next)
    }

    pub fn skip_backward(&mut self) -> Option<&Track> {
        let previous = self.previous_index()?;
        self.current = Some(previous);
        self.tracks.get(previous)
    }

    pub fn advance(&mut self) -> Option<&Track> {
        if self.repeat == RepeatMode::One {
            return self.current_track();
        }
        match self.next_index() {
            Some(next) => {
                self.current = Some(next);
                self.tracks.get(next)
            }
            None => {
                self.current = None;
                None
            }
        }
    }

    fn next_index(&mut self) -> Option<usize> {
        if self.tracks.is_empty() {
            return None;
        }
        if self.shuffle {
            return Some(self.random_other_index());
        }
        let current = self.current?;
        let last = self.tracks.len() - 1;
        match (current < last, self.repeat) {
            (true, _) => Some(current + 1),
            (false, RepeatMode::All) => Some(0),
            (false, RepeatMode::Off | RepeatMode::One) => None,
        }
    }

    fn previous_index(&mut self) -> Option<usize> {
        if self.tracks.is_empty() {
            return None;
        }
        if self.shuffle {
            return Some(self.random_other_index());
        }
        let current = self.current?;
        match (current > 0, self.repeat) {
            (true, _) => Some(current - 1),
            (false, RepeatMode::All) => Some(self.tracks.len() - 1),
            (false, RepeatMode::Off | RepeatMode::One) => None,
        }
    }

    fn random_other_index(&mut self) -> usize {
        let len = self.tracks.len();
        if len <= 1 {
            return 0;
        }
        loop {
            let pick = (splitmix64(&mut self.shuffle_seed) % len as u64) as usize;
            if Some(pick) != self.current {
                return pick;
            }
        }
    }
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: &str) -> Track {
        Track {
            id: id.to_owned(),
            title: id.to_owned(),
            artist: None,
            artist_id: None,
            album: None,
            duration: None,
            thumbnail_url: None,
            credits: Vec::new(),
        }
    }

    fn queue(ids: &[&str]) -> PlaybackQueue {
        PlaybackQueue::new(ids.iter().map(|id| track(id)).collect())
    }

    #[test]
    fn a_fresh_queue_has_nothing_current() {
        let q = queue(&["a", "b"]);

        assert_eq!(q.current_index(), None);
        assert_eq!(q.current_track(), None);
    }

    #[test]
    fn playing_an_out_of_range_index_changes_nothing() {
        let mut q = queue(&["a", "b"]);

        assert_eq!(q.play_at(5), None);
        assert_eq!(q.current_index(), None);
    }

    #[test]
    fn playing_a_track_makes_it_current() {
        let mut q = queue(&["a", "b", "c"]);

        let played = q.play_at(1).cloned();

        assert_eq!(played, Some(track("b")));
        assert_eq!(q.current_index(), Some(1));
    }

    #[test]
    fn skipping_forward_moves_sequentially() {
        let mut q = queue(&["a", "b", "c"]);
        q.play_at(0);

        assert_eq!(q.skip_forward().map(|t| t.id.as_str()), Some("b"));
        assert_eq!(q.skip_forward().map(|t| t.id.as_str()), Some("c"));
    }

    #[test]
    fn skipping_forward_past_the_end_stops_when_repeat_is_off() {
        let mut q = queue(&["a", "b"]);
        q.play_at(1);

        assert_eq!(q.skip_forward(), None);
        assert_eq!(q.current_index(), Some(1));
    }

    #[test]
    fn skipping_forward_past_the_end_wraps_when_repeat_is_all() {
        let mut q = queue(&["a", "b"]);
        q.set_repeat(RepeatMode::All);
        q.play_at(1);

        assert_eq!(q.skip_forward().map(|t| t.id.as_str()), Some("a"));
    }

    #[test]
    fn skipping_backward_before_the_start_stops_when_repeat_is_off() {
        let mut q = queue(&["a", "b"]);
        q.play_at(0);

        assert_eq!(q.skip_backward(), None);
        assert_eq!(q.current_index(), Some(0));
    }

    #[test]
    fn skipping_backward_before_the_start_wraps_when_repeat_is_all() {
        let mut q = queue(&["a", "b"]);
        q.set_repeat(RepeatMode::All);
        q.play_at(0);

        assert_eq!(q.skip_backward().map(|t| t.id.as_str()), Some("b"));
    }

    #[test]
    fn repeat_one_restarts_the_same_track_only_when_it_ends_on_its_own() {
        let mut q = queue(&["a", "b"]);
        q.set_repeat(RepeatMode::One);
        q.play_at(0);

        assert_eq!(q.advance().map(|t| t.id.as_str()), Some("a"));
        assert_eq!(q.current_index(), Some(0));
    }

    #[test]
    fn repeat_one_does_not_stop_an_explicit_skip() {
        let mut q = queue(&["a", "b"]);
        q.set_repeat(RepeatMode::One);
        q.play_at(0);

        assert_eq!(q.skip_forward().map(|t| t.id.as_str()), Some("b"));
    }

    #[test]
    fn advance_without_repeat_one_behaves_like_skip_forward() {
        let mut q = queue(&["a", "b"]);
        q.play_at(0);

        assert_eq!(q.advance().map(|t| t.id.as_str()), Some("b"));
    }

    #[test]
    fn advancing_past_the_end_stops_and_clears_current_when_repeat_is_off() {
        let mut q = queue(&["a", "b"]);
        q.play_at(1);

        assert_eq!(q.advance(), None);
        assert_eq!(q.current_index(), None);
        assert_eq!(q.current_track(), None);
    }

    #[test]
    fn advancing_past_the_end_wraps_when_repeat_is_all() {
        let mut q = queue(&["a", "b"]);
        q.set_repeat(RepeatMode::All);
        q.play_at(1);

        assert_eq!(q.advance().map(|t| t.id.as_str()), Some("a"));
    }

    #[test]
    fn shuffle_never_lands_on_the_already_current_track() {
        let mut q = queue(&["a", "b", "c", "d", "e"]);
        q.set_shuffle(true);
        q.play_at(0);

        for _ in 0..50 {
            let before = q.current_index();
            q.skip_forward();
            assert_ne!(q.current_index(), before);
        }
    }

    #[test]
    fn shuffle_with_one_track_always_lands_on_it() {
        let mut q = queue(&["a"]);
        q.set_shuffle(true);
        q.play_at(0);

        assert_eq!(q.skip_forward().map(|t| t.id.as_str()), Some("a"));
    }

    #[test]
    fn an_empty_queue_never_plays_anything() {
        let mut q = queue(&[]);

        assert_eq!(q.play_at(0), None);
        assert_eq!(q.skip_forward(), None);
        assert_eq!(q.skip_backward(), None);
        assert_eq!(q.advance(), None);
    }

    #[test]
    fn appending_skips_tracks_already_queued() {
        let mut queue = PlaybackQueue::new(vec![track("a"), track("b")]);

        let added = queue.append(vec![track("b"), track("c")]);

        assert_eq!(added, [track("c")]);
        assert_eq!(queue.tracks().len(), 3);
    }

    #[test]
    fn remaining_counts_the_tracks_after_the_current_one() {
        let mut queue = PlaybackQueue::new(vec![track("a"), track("b"), track("c")]);
        assert_eq!(queue.remaining(), 0);

        queue.play_at(0);
        assert_eq!(queue.remaining(), 2);
        queue.play_at(2);
        assert_eq!(queue.remaining(), 0);
    }

    #[test]
    fn peek_next_matches_what_advancing_would_play() {
        let mut queue = PlaybackQueue::new(vec![track("a"), track("b")]);
        queue.play_at(0);
        assert_eq!(queue.peek_next(), Some(1));

        queue.play_at(1);
        assert_eq!(queue.peek_next(), None);
        queue.set_repeat(RepeatMode::All);
        assert_eq!(queue.peek_next(), Some(0));
    }

    #[test]
    fn peek_next_is_unknowable_under_shuffle_or_repeat_one() {
        let mut queue = PlaybackQueue::new(vec![track("a"), track("b")]);
        queue.play_at(0);

        queue.set_shuffle(true);
        assert_eq!(queue.peek_next(), None);
        queue.set_shuffle(false);
        queue.set_repeat(RepeatMode::One);
        assert_eq!(queue.peek_next(), None);
    }
}
