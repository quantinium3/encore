use std::cmp::Ordering;

use gpui_kit::{
    AnyElement, App, ClickEvent, ClipboardItem, Context, Entity, Hsla, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Subscription, Window,
    assets::IconName,
    base::{h_flex, v_flex},
    component::{
        ActiveTheme as _, Disableable as _, Icon, Sizable as _, WindowExt as _,
        button::{Button, ButtonVariants as _},
        input::{Input, InputEvent, InputState},
        menu::{DropdownMenu as _, PopupMenu, PopupMenuItem},
        notification::Notification,
    },
    div, px,
};
use rand::seq::SliceRandom as _;

use crate::components::playlist::header::PlaylistHeader;
use crate::components::playlist::track_item::PlaylistTrackItem;
use crate::player::PlayerModel;
use crate::playlist_detail::{
    PlaylistDetailModel, PlaylistDetailSummary, PlaylistPrivacy, TrackSummary,
};

const SEARCH_WIDTH: f32 = 260.;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortField {
    Position,
    Title,
    Artist,
    Album,
    Duration,
}

impl SortField {
    const ALL: [Self; 5] = [
        Self::Position,
        Self::Title,
        Self::Artist,
        Self::Album,
        Self::Duration,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Position => "Playlist order",
            Self::Title => "Title",
            Self::Artist => "Artist",
            Self::Album => "Album",
            Self::Duration => "Duration",
        }
    }
}

pub struct PlaylistDetailView {
    model: Entity<PlaylistDetailModel>,
    player: Entity<PlayerModel>,
    search_input: Entity<InputState>,
    sort: SortField,
    sort_descending: bool,
    _observe_detail: Subscription,
    _observe_player: Subscription,
    _observe_search: Subscription,
}

impl PlaylistDetailView {
    pub fn new(
        model: Entity<PlaylistDetailModel>,
        player: Entity<PlayerModel>,
        search_input: Entity<InputState>,
        cx: &mut Context<Self>,
    ) -> Self {
        let observe_detail = cx.observe(&model, |_, _, cx| cx.notify());
        let observe_player = cx.observe(&player, |_, _, cx| cx.notify());
        let observe_search = cx.subscribe(&search_input, |_, _, event: &InputEvent, cx| {
            if let InputEvent::Change = event {
                cx.notify();
            }
        });
        Self {
            model,
            player,
            search_input,
            sort: SortField::Position,
            sort_descending: false,
            _observe_detail: observe_detail,
            _observe_player: observe_player,
            _observe_search: observe_search,
        }
    }
}

impl Render for PlaylistDetailView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let detail = self.model.read(cx).detail().cloned();
        let player_state = self.player.read(cx);
        let now_playing = player_state.current_track().map(|track| track.id.clone());
        let status = player_state.status();
        let query = self.search_input.read(cx).value().to_lowercase();

        let playback = Playback {
            now_playing: now_playing.as_ref(),
            now_loading: status.is_loading(),
            now_playing_actively: status.is_playing(),
            player: &self.player,
        };
        let toolbar = Toolbar {
            search_input: &self.search_input,
            query: &query,
            sort: self.sort,
            sort_descending: self.sort_descending,
            view: cx.entity(),
            muted,
            primary_foreground: cx.theme().primary_foreground,
        };

        match detail {
            Some(detail) => render_detail(detail, playback, toolbar).into_any_element(),
            None => div()
                .w_full()
                .min_w_0()
                .p_4()
                .text_sm()
                .text_color(muted)
                .child("Select a playlist to see its tracks.")
                .into_any_element(),
        }
    }
}

struct Playback<'a> {
    now_playing: Option<&'a SharedString>,
    now_loading: bool,
    now_playing_actively: bool,
    player: &'a Entity<PlayerModel>,
}

struct Toolbar<'a> {
    search_input: &'a Entity<InputState>,
    query: &'a str,
    sort: SortField,
    sort_descending: bool,
    view: Entity<PlaylistDetailView>,
    muted: Hsla,
    primary_foreground: Hsla,
}

fn render_detail(
    detail: PlaylistDetailSummary,
    playback: Playback,
    toolbar: Toolbar,
) -> AnyElement {
    let Playback {
        now_playing,
        now_loading,
        now_playing_actively,
        player,
    } = playback;
    let Toolbar {
        search_input,
        query,
        sort,
        sort_descending,
        view,
        muted,
        primary_foreground,
    } = toolbar;

    let mut header = PlaylistHeader::new(detail.title);
    header = match detail.thumbnail_url {
        Some(url) => header.poster(url),
        None => header,
    };
    header = match detail.description {
        Some(description) => header.description(description),
        None => header,
    };
    if let Some(subtitle) = playlist_subtitle(detail.privacy, detail.year.as_deref()) {
        header = header.subtitle(subtitle);
    }
    if let Some(meta) = playlist_meta(detail.track_count, detail.total_duration.as_deref()) {
        header = header.meta(meta);
    }

    let all_tracks = detail.tracks.clone();

    let mut tracks: Vec<TrackSummary> = detail
        .tracks
        .into_iter()
        .filter(|track| matches_query(track, query))
        .collect();
    tracks.sort_by(|a, b| compare_tracks(a, b, sort, sort_descending));
    if sort == SortField::Position && sort_descending {
        tracks.reverse();
    }

    let actions = h_flex()
        .items_center()
        .gap_2()
        .child(play_button(
            all_tracks.clone(),
            player.clone(),
            primary_foreground,
        ))
        .child(shuffle_button(all_tracks, player.clone(), muted))
        .child(share_button(detail.playlist_id, muted));

    let search = Input::new(search_input)
        .w(px(SEARCH_WIDTH))
        .cleanable(true)
        .prefix(Icon::new(IconName::Search).size_3p5().text_color(muted));

    let search_and_sort = h_flex()
        .items_center()
        .gap_2()
        .child(search)
        .child(sort_control(view.clone(), sort, muted))
        .child(direction_toggle(view, sort_descending, muted));

    let controls = h_flex()
        .w_full()
        .items_center()
        .justify_between()
        .child(actions)
        .child(search_and_sort);

    v_flex()
        .w_full()
        .min_w_0()
        .p_4()
        .gap_4()
        .child(header)
        .child(controls)
        .child(
            v_flex()
                .w_full()
                .min_w_0()
                .gap_1()
                .children(tracks.iter().enumerate().map(|(index, track)| {
                    let is_current = now_playing == Some(&track.id);
                    let is_loading = is_current && now_loading;
                    let is_playing = is_current && now_playing_actively;
                    render_track(
                        index,
                        track.clone(),
                        is_current,
                        is_loading,
                        is_playing,
                        tracks.clone(),
                        player,
                    )
                })),
        )
        .into_any_element()
}

fn playlist_subtitle(privacy: Option<PlaylistPrivacy>, year: Option<&str>) -> Option<SharedString> {
    let mut parts = vec!["Playlist"];
    if let Some(privacy) = privacy {
        parts.push(privacy.label());
    }
    if let Some(year) = year {
        parts.push(year);
    }
    Some(SharedString::from(parts.join(" • ")))
}

fn playlist_meta(track_count: Option<usize>, total_duration: Option<&str>) -> Option<SharedString> {
    let count = track_count?;
    let noun = if count == 1 { "track" } else { "tracks" };
    let mut meta = format!("{count} {noun}");
    if let Some(duration) = total_duration {
        meta.push_str(" • ");
        meta.push_str(duration);
    }
    Some(SharedString::from(meta))
}

pub(crate) fn play_button(
    tracks: Vec<TrackSummary>,
    player: Entity<PlayerModel>,
    foreground: Hsla,
) -> Button {
    Button::new("playlist-play")
        .cursor_pointer()
        .primary()
        .rounded(px(6.))
        .icon(Icon::new(IconName::Play).size_4().text_color(foreground))
        .label("Play")
        .disabled(tracks.is_empty())
        .on_click(move |_, _, cx| {
            let tracks = tracks.clone();
            player.update(cx, |model, cx| model.request_play_queue(tracks, 0, cx));
        })
}

pub(crate) fn shuffle_button(
    tracks: Vec<TrackSummary>,
    player: Entity<PlayerModel>,
    muted: Hsla,
) -> Button {
    Button::new("playlist-shuffle")
        .cursor_pointer()
        .ghost()
        .rounded(px(6.))
        .icon(Icon::new(IconName::Shuffle).size_4().text_color(muted))
        .label("Shuffle")
        .disabled(tracks.is_empty())
        .on_click(move |_, _, cx| {
            let mut shuffled = tracks.clone();
            shuffled.shuffle(&mut rand::rng());
            player.update(cx, |model, cx| model.request_play_queue(shuffled, 0, cx));
        })
}

fn share_button(playlist_id: SharedString, muted: Hsla) -> Button {
    Button::new("playlist-share")
        .cursor_pointer()
        .ghost()
        .icon(Icon::new(IconName::Share2).size_4().text_color(muted))
        .on_click(move |_, window, cx| {
            let url = format!("https://music.youtube.com/playlist?list={playlist_id}");
            cx.write_to_clipboard(ClipboardItem::new_string(url));
            window.push_notification(Notification::success("Playlist link copied"), cx);
        })
}

fn sort_control(
    view: Entity<PlaylistDetailView>,
    sort: SortField,
    muted: Hsla,
) -> impl IntoElement {
    Button::new("playlist-sort")
        .cursor_pointer()
        .ghost()
        .small()
        .icon(Icon::new(IconName::ArrowUpDown).size_4().text_color(muted))
        .dropdown_menu(move |menu, _, _| sort_menu(menu, &view, sort))
}

fn sort_menu(menu: PopupMenu, view: &Entity<PlaylistDetailView>, sort: SortField) -> PopupMenu {
    SortField::ALL.into_iter().fold(menu, |menu, field| {
        let view = view.clone();
        menu.item(
            PopupMenuItem::new(field.label())
                .checked(field == sort)
                .on_click(move |_, _, cx| {
                    view.update(cx, |this, cx| {
                        this.sort = field;
                        cx.notify();
                    });
                }),
        )
    })
}

fn direction_toggle(view: Entity<PlaylistDetailView>, descending: bool, muted: Hsla) -> Button {
    let icon = match descending {
        true => IconName::ArrowDownWideNarrow,
        false => IconName::ArrowUpNarrowWide,
    };
    Button::new("playlist-sort-direction")
        .cursor_pointer()
        .ghost()
        .small()
        .icon(Icon::new(icon).size_4().text_color(muted))
        .on_click(move |_, _, cx| {
            view.update(cx, |this, cx| {
                this.sort_descending = !this.sort_descending;
                cx.notify();
            });
        })
}

fn matches_query(track: &TrackSummary, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    track.title.to_lowercase().contains(query)
        || contains(track.artist.as_deref(), query)
        || contains(track.album.as_deref(), query)
}

fn contains(field: Option<&str>, query: &str) -> bool {
    field.is_some_and(|field| field.to_lowercase().contains(query))
}

fn compare_tracks(
    a: &TrackSummary,
    b: &TrackSummary,
    field: SortField,
    descending: bool,
) -> Ordering {
    match field {
        SortField::Position => Ordering::Equal,
        SortField::Title => compare_text(Some(&a.title), Some(&b.title), descending),
        SortField::Artist => compare_text(a.artist.as_deref(), b.artist.as_deref(), descending),
        SortField::Album => compare_text(a.album.as_deref(), b.album.as_deref(), descending),
        SortField::Duration => {
            compare_duration(a.duration.as_deref(), b.duration.as_deref(), descending)
        }
    }
}

fn compare_text(
    a: Option<impl AsRef<str>>,
    b: Option<impl AsRef<str>>,
    descending: bool,
) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => {
            let ordering = a.as_ref().to_lowercase().cmp(&b.as_ref().to_lowercase());
            if descending {
                ordering.reverse()
            } else {
                ordering
            }
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn compare_duration(a: Option<&str>, b: Option<&str>, descending: bool) -> Ordering {
    match (a.and_then(duration_secs), b.and_then(duration_secs)) {
        (Some(a), Some(b)) => {
            let ordering = a.cmp(&b);
            if descending {
                ordering.reverse()
            } else {
                ordering
            }
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn duration_secs(duration: &str) -> Option<u64> {
    duration.split(':').try_fold(0u64, |total, part| {
        total.checked_mul(60)?.checked_add(part.parse().ok()?)
    })
}

pub(crate) fn render_track(
    index: usize,
    track: TrackSummary,
    is_current: bool,
    is_loading: bool,
    is_playing: bool,
    tracks: Vec<TrackSummary>,
    player: &Entity<PlayerModel>,
) -> AnyElement {
    build_track(index, true, track, is_current, is_loading, is_playing, tracks, player)
}

pub(crate) fn render_unnumbered_track(
    index: usize,
    track: TrackSummary,
    is_current: bool,
    is_loading: bool,
    is_playing: bool,
    tracks: Vec<TrackSummary>,
    player: &Entity<PlayerModel>,
) -> AnyElement {
    build_track(index, false, track, is_current, is_loading, is_playing, tracks, player)
}

#[allow(clippy::too_many_arguments)]
fn build_track(
    index: usize,
    numbered: bool,
    track: TrackSummary,
    is_current: bool,
    is_loading: bool,
    is_playing: bool,
    tracks: Vec<TrackSummary>,
    player: &Entity<PlayerModel>,
) -> AnyElement {
    let item = PlaylistTrackItem::new(format!("track-{}", track.id), track.title.clone())
        .active(is_current)
        .loading(is_loading)
        .playing(is_playing);
    let item = match numbered {
        true => item.index(index + 1),
        false => item.overlay(),
    };
    let item = match track.artist {
        Some(artist) => item.artist(artist),
        None => item,
    };
    let item = match track.artist_id {
        Some(artist_id) => item.on_artist_click(move |cx: &mut App| {
            if let Some(navigator) = cx.try_global::<crate::artist::ArtistNavigator>() {
                let model = navigator.0.clone();
                model.update(cx, |model, cx| model.select(artist_id.clone(), cx));
            }
        }),
        None => item,
    };
    let item = match track.duration {
        Some(duration) => item.duration(duration),
        None => item,
    };
    let item = match track.thumbnail_url {
        Some(url) => item.thumbnail(url),
        None => item,
    };

    let player = player.clone();
    item.on_click(move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
        let tracks = tracks.clone();
        player.update(cx, |model, cx| model.request_track_click(tracks, index, cx));
    })
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: &str, title: &str, artist: Option<&str>, duration: Option<&str>) -> TrackSummary {
        TrackSummary {
            id: id.into(),
            title: title.into(),
            artist: artist.map(Into::into),
            artist_id: None,
            album: None,
            duration: duration.map(Into::into),
            thumbnail_url: None,
            credits: Vec::new(),
        }
    }

    #[test]
    fn a_query_matches_the_title_case_insensitively() {
        let t = track("1", "Yodaka", None, None);
        assert!(matches_query(&t, "yod"));
        assert!(!matches_query(&t, "nope"));
    }

    #[test]
    fn a_query_also_matches_the_artist() {
        let t = track("1", "Yodaka", Some("Kenshi Yonezu"), None);
        assert!(matches_query(&t, "yonezu"));
    }

    #[test]
    fn an_empty_query_matches_everything() {
        let t = track("1", "Yodaka", None, None);
        assert!(matches_query(&t, ""));
    }

    #[test]
    fn durations_are_parsed_as_minutes_and_seconds() {
        assert_eq!(duration_secs("3:01"), Some(181));
        assert_eq!(duration_secs("1:02:03"), Some(3723));
        assert_eq!(duration_secs("not a duration"), None);
    }

    #[test]
    fn tracks_without_a_duration_sort_after_ones_with_one_either_direction() {
        let with = track("1", "A", None, Some("3:00"));
        let without = track("2", "B", None, None);
        assert_eq!(
            compare_duration(with.duration.as_deref(), without.duration.as_deref(), false),
            Ordering::Less
        );
        assert_eq!(
            compare_duration(with.duration.as_deref(), without.duration.as_deref(), true),
            Ordering::Less
        );
    }

    #[test]
    fn shorter_durations_sort_before_longer_ones_unless_descending() {
        assert_eq!(
            compare_duration(Some("1:00"), Some("2:00"), false),
            Ordering::Less
        );
        assert_eq!(
            compare_duration(Some("1:00"), Some("2:00"), true),
            Ordering::Greater
        );
    }

    #[test]
    fn titles_sort_case_insensitively() {
        let a = track("1", "banana", None, None);
        let b = track("2", "Apple", None, None);
        assert_eq!(
            compare_tracks(&a, &b, SortField::Title, false),
            Ordering::Greater
        );
    }
}
