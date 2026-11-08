use gpui_kit::{Context, EventEmitter};

use crate::playlist_detail::TrackSummary;

pub const MAX_VOLUME_PERCENT: i64 = 200;

pub const DEFAULT_VOLUME_PERCENT: i64 = 100;

#[derive(Debug, Clone, PartialEq)]
pub enum PlayerEvent {
    PlayQueueRequested {
        tracks: Vec<TrackSummary>,
        start_index: usize,
    },
    PlayPauseRequested,
    NextRequested,
    PreviousRequested,
    ShuffleToggleRequested,
    RepeatToggleRequested,
    VolumeRequested(i64),
    SeekRequested(f64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaybackStatus {
    #[default]
    Idle,
    Loading,
    Playing,
    Paused,
}

impl PlaybackStatus {
    pub fn is_loading(self) -> bool {
        self == Self::Loading
    }

    pub fn is_playing(self) -> bool {
        self == Self::Playing
    }

    fn after_queue_change(has_current: bool) -> Self {
        match has_current {
            true => Self::Loading,
            false => Self::Idle,
        }
    }

    fn after_media_playing(self, has_current: bool, playing: bool) -> Self {
        if !has_current {
            return self;
        }
        match (self, playing) {
            (Self::Loading, false) => Self::Loading,
            (_, true) => Self::Playing,
            (_, false) => Self::Paused,
        }
    }
}

pub struct PlayerModel {
    queue: Vec<TrackSummary>,
    current_index: Option<usize>,
    status: PlaybackStatus,
    position_secs: f64,
    duration_secs: f64,
    volume_percent: i64,
    shuffle: bool,
    repeat: bool,
}

impl Default for PlayerModel {
    fn default() -> Self {
        Self {
            queue: Vec::new(),
            current_index: None,
            status: PlaybackStatus::default(),
            position_secs: 0.,
            duration_secs: 0.,
            volume_percent: DEFAULT_VOLUME_PERCENT,
            shuffle: false,
            repeat: false,
        }
    }
}

impl EventEmitter<PlayerEvent> for PlayerModel {}

impl PlayerModel {
    pub fn current_track(&self) -> Option<&TrackSummary> {
        self.current_index.and_then(|index| self.queue.get(index))
    }

    pub fn queue(&self) -> &[TrackSummary] {
        &self.queue
    }

    pub fn current_index(&self) -> Option<usize> {
        self.current_index
    }

    pub fn status(&self) -> PlaybackStatus {
        self.status
    }

    pub fn position_secs(&self) -> f64 {
        self.position_secs
    }

    pub fn duration_secs(&self) -> f64 {
        self.duration_secs
    }

    pub fn volume_percent(&self) -> i64 {
        self.volume_percent
    }

    pub fn shuffle(&self) -> bool {
        self.shuffle
    }

    pub fn repeat(&self) -> bool {
        self.repeat
    }

    pub fn set_queue(
        &mut self,
        queue: Vec<TrackSummary>,
        current_index: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        self.queue = queue;
        self.current_index = current_index;
        self.position_secs = 0.;
        self.duration_secs = 0.;
        self.status = PlaybackStatus::after_queue_change(current_index.is_some());
        cx.notify();
    }

    pub fn restore_session(
        &mut self,
        queue: Vec<TrackSummary>,
        current_index: usize,
        position_secs: f64,
        duration_secs: f64,
        cx: &mut Context<Self>,
    ) {
        if current_index >= queue.len() {
            return;
        }
        self.queue = queue;
        self.current_index = Some(current_index);
        self.position_secs = position_secs.max(0.);
        self.duration_secs = duration_secs.max(0.);
        self.status = PlaybackStatus::Paused;
        cx.notify();
    }

    pub fn append_queue(&mut self, tracks: Vec<TrackSummary>, cx: &mut Context<Self>) {
        if tracks.is_empty() {
            return;
        }
        self.queue.extend(tracks);
        cx.notify();
    }

    pub fn set_playing(&mut self, playing: bool, cx: &mut Context<Self>) {
        let status = self
            .status
            .after_media_playing(self.current_index.is_some(), playing);
        if self.status != status {
            self.status = status;
            cx.notify();
        }
    }

    pub fn set_position_secs(&mut self, position_secs: f64, cx: &mut Context<Self>) {
        self.position_secs = position_secs;
        cx.notify();
    }

    pub fn set_duration_secs(&mut self, duration_secs: f64, cx: &mut Context<Self>) {
        self.duration_secs = duration_secs;
        cx.notify();
    }

    pub fn set_volume_percent(&mut self, percent: i64, cx: &mut Context<Self>) {
        self.volume_percent = percent.clamp(0, MAX_VOLUME_PERCENT);
        cx.notify();
    }

    pub fn set_shuffle(&mut self, shuffle: bool, cx: &mut Context<Self>) {
        if self.shuffle != shuffle {
            self.shuffle = shuffle;
            cx.notify();
        }
    }

    pub fn set_repeat(&mut self, repeat: bool, cx: &mut Context<Self>) {
        if self.repeat != repeat {
            self.repeat = repeat;
            cx.notify();
        }
    }

    pub fn request_play_queue(
        &mut self,
        tracks: Vec<TrackSummary>,
        start_index: usize,
        cx: &mut Context<Self>,
    ) {
        if start_index < tracks.len() {
            cx.emit(PlayerEvent::PlayQueueRequested {
                tracks,
                start_index,
            });
        }
    }

    pub fn request_track_click(
        &mut self,
        tracks: Vec<TrackSummary>,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        match self.track_click_is_toggle(&tracks, index) {
            true => cx.emit(PlayerEvent::PlayPauseRequested),
            false => self.request_play_queue(tracks, index, cx),
        }
    }

    pub fn request_seek(&mut self, position_secs: f64, cx: &mut Context<Self>) {
        if self.current_index.is_none() || !position_secs.is_finite() {
            return;
        }
        let position_secs = position_secs.clamp(0., self.duration_secs.max(0.));
        self.position_secs = position_secs;
        cx.emit(PlayerEvent::SeekRequested(position_secs));
        cx.notify();
    }

    pub fn request_play_pause(&mut self, cx: &mut Context<Self>) {
        if self.play_pause_event().is_some() {
            cx.emit(PlayerEvent::PlayPauseRequested);
        }
    }

    pub fn request_next(&mut self, cx: &mut Context<Self>) {
        if self.skip_event().is_some() {
            cx.emit(PlayerEvent::NextRequested);
        }
    }

    pub fn request_previous(&mut self, cx: &mut Context<Self>) {
        if self.skip_event().is_some() {
            cx.emit(PlayerEvent::PreviousRequested);
        }
    }

    pub fn request_shuffle_toggle(&mut self, cx: &mut Context<Self>) {
        cx.emit(PlayerEvent::ShuffleToggleRequested);
    }

    pub fn request_repeat_toggle(&mut self, cx: &mut Context<Self>) {
        cx.emit(PlayerEvent::RepeatToggleRequested);
    }

    pub fn request_volume(&mut self, percent: i64, cx: &mut Context<Self>) {
        cx.emit(PlayerEvent::VolumeRequested(
            percent.clamp(0, MAX_VOLUME_PERCENT),
        ));
    }

    fn play_pause_event(&self) -> Option<()> {
        matches!(
            self.status,
            PlaybackStatus::Playing | PlaybackStatus::Paused
        )
        .then_some(())
    }

    fn track_click_is_toggle(&self, tracks: &[TrackSummary], index: usize) -> bool {
        let clicked_id = tracks.get(index).map(|track| &track.id);
        let is_current_track =
            clicked_id.is_some() && clicked_id == self.current_track().map(|track| &track.id);
        is_current_track && self.play_pause_event().is_some()
    }

    fn skip_event(&self) -> Option<()> {
        (!self.queue.is_empty()).then_some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: &str) -> TrackSummary {
        TrackSummary {
            id: id.to_owned().into(),
            title: id.to_owned().into(),
            artist: None,
            artist_id: None,
            album: None,
            duration: None,
            thumbnail_url: None,
            credits: Vec::new(),
        }
    }

    fn model(ids: &[&str], current_index: Option<usize>, status: PlaybackStatus) -> PlayerModel {
        PlayerModel {
            queue: ids.iter().map(|id| track(id)).collect(),
            current_index,
            status,
            ..PlayerModel::default()
        }
    }

    #[test]
    fn a_fresh_model_has_no_current_track_and_is_idle() {
        let model = PlayerModel::default();

        assert_eq!(model.current_track(), None);
        assert_eq!(model.status(), PlaybackStatus::Idle);
    }

    #[test]
    fn play_pause_is_requestable_only_while_playing_or_paused() {
        let idle = PlayerModel::default();
        let loading = model(&["a"], Some(0), PlaybackStatus::Loading);
        let playing = model(&["a"], Some(0), PlaybackStatus::Playing);
        let paused = model(&["a"], Some(0), PlaybackStatus::Paused);

        assert_eq!(idle.play_pause_event(), None);
        assert_eq!(loading.play_pause_event(), None);
        assert_eq!(playing.play_pause_event(), Some(()));
        assert_eq!(paused.play_pause_event(), Some(()));
    }

    #[test]
    fn picking_a_track_starts_loading() {
        assert_eq!(
            PlaybackStatus::after_queue_change(true),
            PlaybackStatus::Loading
        );
    }

    #[test]
    fn losing_the_current_track_goes_idle() {
        assert_eq!(
            PlaybackStatus::after_queue_change(false),
            PlaybackStatus::Idle
        );
    }

    #[test]
    fn mpv_starting_the_track_leaves_loading() {
        assert_eq!(
            PlaybackStatus::Loading.after_media_playing(true, true),
            PlaybackStatus::Playing
        );
    }

    #[test]
    fn a_paused_report_while_still_loading_is_swallowed() {
        assert_eq!(
            PlaybackStatus::Loading.after_media_playing(true, false),
            PlaybackStatus::Loading
        );
    }

    #[test]
    fn toggling_while_playing_or_paused_flips_between_the_two() {
        assert_eq!(
            PlaybackStatus::Playing.after_media_playing(true, false),
            PlaybackStatus::Paused
        );
        assert_eq!(
            PlaybackStatus::Paused.after_media_playing(true, true),
            PlaybackStatus::Playing
        );
    }

    #[test]
    fn a_stale_media_report_with_nothing_current_is_ignored() {
        assert_eq!(
            PlaybackStatus::Idle.after_media_playing(false, false),
            PlaybackStatus::Idle
        );
    }

    #[test]
    fn skipping_is_requestable_only_with_a_nonempty_queue() {
        let empty = PlayerModel::default();
        let nonempty = model(&["a", "b"], Some(0), PlaybackStatus::Playing);

        assert_eq!(empty.skip_event(), None);
        assert_eq!(nonempty.skip_event(), Some(()));
    }

    #[test]
    fn clicking_the_already_playing_track_toggles_instead_of_restarting() {
        let model = model(&["a", "b"], Some(0), PlaybackStatus::Playing);
        let tracks: Vec<TrackSummary> = ["a", "b"].iter().map(|id| track(id)).collect();

        assert!(model.track_click_is_toggle(&tracks, 0));
    }

    #[test]
    fn clicking_the_already_paused_track_also_toggles() {
        let model = model(&["a", "b"], Some(0), PlaybackStatus::Paused);
        let tracks: Vec<TrackSummary> = ["a", "b"].iter().map(|id| track(id)).collect();

        assert!(model.track_click_is_toggle(&tracks, 0));
    }

    #[test]
    fn clicking_a_different_track_starts_it_instead_of_toggling() {
        let model = model(&["a", "b"], Some(0), PlaybackStatus::Playing);
        let tracks: Vec<TrackSummary> = ["a", "b"].iter().map(|id| track(id)).collect();

        assert!(!model.track_click_is_toggle(&tracks, 1));
    }

    #[test]
    fn clicking_the_current_track_while_it_is_still_loading_does_not_toggle() {
        let model = model(&["a", "b"], Some(0), PlaybackStatus::Loading);
        let tracks: Vec<TrackSummary> = ["a", "b"].iter().map(|id| track(id)).collect();

        assert!(!model.track_click_is_toggle(&tracks, 0));
    }
}
