use gpui_kit::{
    App, AppContext as _, ClickEvent, Context, Entity, IntoElement, ParentElement as _, Render,
    SharedString, Subscription, Window,
    component::{
        Placement, WindowExt as _,
        slider::{SliderEvent, SliderState},
    },
};

use crate::components::player::bar::PlayerBar;
use crate::components::player::queue_panel::QueuePanelState;
use crate::components::song_info::SongInfoCard;
use crate::player::{MAX_VOLUME_PERCENT, PlayerModel};

pub struct PlayerBarView {
    model: Entity<PlayerModel>,
    queue_state: Entity<QueuePanelState>,
    volume_slider: Entity<SliderState>,
    seek_slider: Entity<SliderState>,
    seeking: bool,
    _observe_seek_slider: Subscription,
    _observe: Subscription,
    _observe_volume_slider: Subscription,
}

impl PlayerBarView {
    pub fn new(
        model: Entity<PlayerModel>,
        queue_state: Entity<QueuePanelState>,
        cx: &mut Context<Self>,
    ) -> Self {
        let observe = cx.observe(&model, |_, _, cx| cx.notify());

        let initial_volume = model.read(cx).volume_percent() as f32;
        let volume_slider = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(MAX_VOLUME_PERCENT as f32)
                .step(1.)
                .default_value(initial_volume)
        });
        let observe_volume_slider =
            cx.subscribe(&volume_slider, |this, _, event: &SliderEvent, cx| {
                if let SliderEvent::Change(value) = event {
                    let percent = value.start().round() as i64;
                    this.model
                        .update(cx, |model, cx| model.request_volume(percent, cx));
                }
            });

        let seek_slider =
            cx.new(|_| SliderState::new().min(0.).max(1.).step(0.001).default_value(0.));
        let observe_seek_slider =
            cx.subscribe(&seek_slider, |this, _, event: &SliderEvent, cx| match event {
                SliderEvent::Change(_) => {
                    this.seeking = true;
                    cx.notify();
                }
                SliderEvent::Release(value) => {
                    this.seeking = false;
                    let fraction = value.start() as f64;
                    this.model.update(cx, |model, cx| {
                        let target = fraction * model.duration_secs();
                        model.request_seek(target, cx)
                    });
                }
            });

        Self {
            model,
            queue_state,
            volume_slider,
            seek_slider,
            seeking: false,
            _observe_seek_slider: observe_seek_slider,
            _observe: observe,
            _observe_volume_slider: observe_volume_slider,
        }
    }
}

impl Render for PlayerBarView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let model = self.model.clone();
        let (position_now, duration_now) = {
            let state = model.read(cx);
            (state.position_secs(), state.duration_secs())
        };
        let live_progress = progress_ratio(position_now, duration_now);
        let shown_progress = match self.seeking {
            true => self.seek_slider.read(cx).value().start(),
            false => {
                if (self.seek_slider.read(cx).value().start() - live_progress).abs() > 0.0005 {
                    self.seek_slider
                        .update(cx, |slider, cx| slider.set_value(live_progress, window, cx));
                }
                live_progress
            }
        };
        let shown_position = shown_progress as f64 * duration_now;
        let state = model.read(cx);
        let track = state.current_track().cloned();
        let title = track
            .as_ref()
            .map(|track| track.title.clone())
            .unwrap_or_else(|| SharedString::from("Nothing playing"));

        let status = state.status();
        let duration = state.duration_secs();
        let mut bar = PlayerBar::new("player-bar", title)
            .loading(status.is_loading())
            .playing(status.is_playing())
            .shuffle_active(state.shuffle())
            .repeat_active(state.repeat())
            .position(format_time(shown_position))
            .duration(format_time(duration))
            .progress(shown_progress)
            .seek_slider(&self.seek_slider)
            .volume_slider(&self.volume_slider);

        if let Some(track) = &track {
            if let Some(artist) = &track.artist {
                bar = bar.artist(artist.clone());
            }
            if let Some(thumbnail) = &track.thumbnail_url {
                bar = bar.thumbnail(thumbnail.clone());
            }
            if let Some(artist_id) = track.artist_id.clone() {
                bar = bar.on_artist(move |cx: &mut App| {
                    if let Some(navigator) = cx.try_global::<crate::artist::ArtistNavigator>() {
                        let model = navigator.0.clone();
                        model.update(cx, |model, cx| model.select(artist_id.clone(), cx));
                    }
                });
            }
        }

        bar.on_queue(toggle_queue(&self.queue_state))
            .on_info(open_song_info(&model))
            .on_play_pause(control(&model, PlayerModel::request_play_pause))
            .on_previous(control(&model, PlayerModel::request_previous))
            .on_next(control(&model, PlayerModel::request_next))
            .on_shuffle(control(&model, PlayerModel::request_shuffle_toggle))
            .on_repeat(control(&model, PlayerModel::request_repeat_toggle))
    }
}

fn control(
    model: &Entity<PlayerModel>,
    request: fn(&mut PlayerModel, &mut Context<PlayerModel>),
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let model = model.clone();
    move |_, _, cx| {
        model.update(cx, |model, cx| request(model, cx));
    }
}

fn toggle_queue(
    state: &Entity<QueuePanelState>,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let state = state.clone();
    move |_, _, cx| {
        state.update(cx, |state, cx| state.toggle(cx));
    }
}

fn open_song_info(
    model: &Entity<PlayerModel>,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let model = model.clone();
    move |_, window, cx| {
        let model = model.clone();
        window.open_sheet_at(Placement::Right, cx, move |sheet, _, cx| {
            let state = model.read(cx);
            let track = state.current_track().cloned();
            let title = track
                .as_ref()
                .map(|track| track.title.clone())
                .unwrap_or_else(|| SharedString::from("Nothing playing"));

            let mut card = SongInfoCard::new(title);

            if let Some(track) = &track {
                if let Some(artist) = &track.artist {
                    card = card.artist(artist.clone());
                }
                if let Some(album) = &track.album {
                    card = card.album(album.clone());
                }
                if let Some(poster) = &track.thumbnail_url {
                    card = card.poster(poster.clone());
                }
                if !track.credits.is_empty() {
                    card = card.credits(track.credits.clone());
                }
            }

            sheet.title("Song Info").child(card)
        });
    }
}

fn format_time(seconds: f64) -> SharedString {
    if !seconds.is_finite() || seconds < 0. {
        return SharedString::from("0:00");
    }
    let total = seconds.round() as u64;
    SharedString::from(format!("{}:{:02}", total / 60, total % 60))
}

fn progress_ratio(position: f64, duration: f64) -> f32 {
    if !duration.is_finite() || duration <= 0. {
        return 0.;
    }
    (position / duration).clamp(0., 1.) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_are_formatted_as_minutes_and_zero_padded_seconds() {
        assert_eq!(format_time(0.), "0:00");
        assert_eq!(format_time(5.), "0:05");
        assert_eq!(format_time(65.), "1:05");
        assert_eq!(format_time(3725.), "62:05");
    }

    #[test]
    fn a_non_finite_or_negative_time_reads_as_zero() {
        assert_eq!(format_time(f64::NAN), "0:00");
        assert_eq!(format_time(f64::INFINITY), "0:00");
        assert_eq!(format_time(-1.), "0:00");
    }

    #[test]
    fn progress_is_the_position_over_the_duration() {
        assert_eq!(progress_ratio(30., 120.), 0.25);
        assert_eq!(progress_ratio(0., 0.), 0.);
    }

    #[test]
    fn progress_is_clamped_to_a_fraction() {
        assert_eq!(progress_ratio(-10., 100.), 0.);
        assert_eq!(progress_ratio(200., 100.), 1.);
    }
}
