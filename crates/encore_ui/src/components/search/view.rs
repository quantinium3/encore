use gpui_kit::{
    AnyElement, App, ClickEvent, Context, Entity, FontWeight, Hsla, IntoElement, ObjectFit,
    ParentElement as _, Render, SharedString, Styled as _, StyledImage as _, Subscription, Window,
    assets::IconName,
    base::{h_flex, v_flex},
    component::{
        ActiveTheme as _, Icon, Sizable as _, Size,
        button::{Button, ButtonRounded, ButtonVariants as _},
        spinner::Spinner,
    },
    div, img,
    prelude::FluentBuilder as _,
    px,
};

use crate::artist::{ArtistModel, CardSummary};
use crate::components::playlist::detail::render_unnumbered_track as render_track;
use crate::player::PlayerModel;
use crate::playlists::PlaylistsModel;
use crate::search::{MixedItemSummary, MixedResultsSummary, SearchKind, SearchModel, SearchState};

pub struct SearchView {
    model: Entity<SearchModel>,
    player: Entity<PlayerModel>,
    playlists: Entity<PlaylistsModel>,
    artist: Entity<ArtistModel>,
    _observe_model: Subscription,
    _observe_player: Subscription,
}

impl SearchView {
    pub fn new(
        model: Entity<SearchModel>,
        player: Entity<PlayerModel>,
        playlists: Entity<PlaylistsModel>,
        artist: Entity<ArtistModel>,
        cx: &mut Context<Self>,
    ) -> Self {
        let observe_model = cx.observe(&model, |_, _, cx| cx.notify());
        let observe_player = cx.observe(&player, |_, _, cx| cx.notify());
        Self {
            model,
            player,
            playlists,
            artist,
            _observe_model: observe_model,
            _observe_player: observe_player,
        }
    }
}

impl Render for SearchView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let foreground = cx.theme().foreground;
        let model = self.model.read(cx);
        let query = model.query().clone();
        let kind = model.kind();
        let state = model.state().clone();
        let player_state = self.player.read(cx);
        let now_playing = player_state.current_track().map(|track| track.id.clone());
        let status = player_state.status();

        let heading = div()
            .text_xl()
            .text_color(foreground)
            .child(SharedString::from(format!("Results for “{query}”")));

        let filters = h_flex()
            .flex_wrap()
            .gap_2()
            .children(SearchKind::ALL.map(|option| {
                let model = self.model.clone();
                Button::new(format!("search-filter-{}", option.label()))
                    .cursor_pointer()
                    .label(option.label())
                    .compact()
                    .rounded(ButtonRounded::Large)
                    .secondary()
                    .toggled(option == kind)
                    .on_click(move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                        model.update(cx, |model, cx| model.set_kind(option, cx));
                    })
            }));

        let body = match state {
            SearchState::Closed => div().into_any_element(),
            SearchState::Loading => div()
                .py_8()
                .flex()
                .justify_center()
                .child(Spinner::new().with_size(Size::Medium).color(muted))
                .into_any_element(),
            SearchState::Failed(message) => div()
                .text_sm()
                .text_color(muted)
                .child(message)
                .into_any_element(),
            SearchState::All(mixed) if mixed.top_result.is_none() && mixed.items.is_empty() => {
                empty_message(muted, kind)
            }
            SearchState::Songs(tracks) if tracks.is_empty() => empty_message(muted, kind),
            SearchState::Videos(tracks) if tracks.is_empty() => empty_message(muted, kind),
            SearchState::Albums(cards) if cards.is_empty() => empty_message(muted, kind),
            SearchState::Artists(cards) if cards.is_empty() => empty_message(muted, kind),
            SearchState::Playlists(cards) if cards.is_empty() => empty_message(muted, kind),
            SearchState::All(mixed) => render_mixed(
                cx.theme().secondary,
                (cx.theme().muted_foreground, cx.theme().muted),
                mixed,
                now_playing.as_ref(),
                status.is_loading(),
                status.is_playing(),
                &self.player,
                &self.playlists,
                &self.artist,
            ),
            SearchState::Songs(tracks) | SearchState::Videos(tracks) => v_flex()
                .w_full()
                .min_w_0()
                .gap_1()
                .children(tracks.iter().enumerate().map(|(index, track)| {
                    let is_current = now_playing.as_ref() == Some(&track.id);
                    render_track(
                        index,
                        track.clone(),
                        is_current,
                        is_current && status.is_loading(),
                        is_current && status.is_playing(),
                        tracks.clone(),
                        &self.player,
                    )
                }))
                .into_any_element(),
            SearchState::Albums(cards) => {
                card_rows(cards, "Album", false, self.playlists.clone(), None)
            }
            SearchState::Playlists(cards) => {
                card_rows(cards, "Playlist", false, self.playlists.clone(), None)
            }
            SearchState::Artists(cards) => card_rows(
                cards,
                "Artist",
                true,
                self.playlists.clone(),
                Some(self.artist.clone()),
            ),
        };

        v_flex()
            .w_full()
            .min_w_0()
            .p_4()
            .gap_4()
            .child(heading)
            .child(filters)
            .child(body)
    }
}

#[allow(clippy::too_many_arguments)]
fn render_mixed(
    secondary: Hsla,
    colors: (Hsla, Hsla),
    mixed: MixedResultsSummary,
    now_playing: Option<&SharedString>,
    now_loading: bool,
    now_playing_actively: bool,
    player: &Entity<PlayerModel>,
    playlists: &Entity<PlaylistsModel>,
    artist: &Entity<ArtistModel>,
) -> AnyElement {
    let top_result = mixed.top_result.map(|top_result| {
        let picks = v_flex().flex_1().min_w_0().gap_1().children(
            top_result
                .quick_picks
                .into_iter()
                .enumerate()
                .map(|(index, track)| {
                    let is_current = now_playing == Some(&track.id);
                    render_track(
                        index,
                        track.clone(),
                        is_current,
                        is_current && now_loading,
                        is_current && now_playing_actively,
                        vec![track],
                        player,
                    )
                }),
        );
        let lead = render_top_lead(colors, top_result.item, playlists, artist, player);
        h_flex()
            .w_full()
            .max_w(px(820.))
            .min_w_0()
            .items_center()
            .gap_4()
            .p_4()
            .rounded_lg()
            .bg(secondary)
            .child(lead)
            .child(picks)
    });

    v_flex()
        .w_full()
        .min_w_0()
        .gap_4()
        .children(top_result)
        .child(v_flex().w_full().min_w_0().gap_1().children(
            mixed.items.into_iter().enumerate().map(|(index, item)| {
                render_mixed_item(
                    index,
                    item,
                    now_playing,
                    now_loading,
                    now_playing_actively,
                    player,
                    playlists,
                    artist,
                )
            }),
        ))
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn render_mixed_item(
    index: usize,
    item: MixedItemSummary,
    now_playing: Option<&SharedString>,
    now_loading: bool,
    now_playing_actively: bool,
    player: &Entity<PlayerModel>,
    playlists: &Entity<PlaylistsModel>,
    artist: &Entity<ArtistModel>,
) -> AnyElement {
    match item {
        MixedItemSummary::Track(track) => {
            let is_current = now_playing == Some(&track.id);
            render_track(
                index,
                track.clone(),
                is_current,
                is_current && now_loading,
                is_current && now_playing_actively,
                vec![track],
                player,
            )
        }
        MixedItemSummary::Album(card) => card_row(card, "Album", false, select_playlist(playlists)),
        MixedItemSummary::Playlist(card) => {
            card_row(card, "Playlist", false, select_playlist(playlists))
        }
        MixedItemSummary::Artist(card) => card_row(card, "Artist", true, select_artist(artist)),
    }
}

type Select = Box<dyn Fn(SharedString, &mut App) + 'static>;

fn select_playlist(playlists: &Entity<PlaylistsModel>) -> Select {
    let playlists = playlists.clone();
    Box::new(move |id, cx| playlists.update(cx, |model, cx| model.select(id, cx)))
}

fn select_artist(artist: &Entity<ArtistModel>) -> Select {
    let artist = artist.clone();
    Box::new(move |id, cx| artist.update(cx, |model, cx| model.select(id, cx)))
}

fn card_rows(
    cards: Vec<CardSummary>,
    kind: &'static str,
    circular: bool,
    playlists: Entity<PlaylistsModel>,
    artist: Option<Entity<ArtistModel>>,
) -> AnyElement {
    v_flex()
        .w_full()
        .min_w_0()
        .gap_1()
        .children(cards.into_iter().map(|card| {
            let select = match &artist {
                Some(artist) => select_artist(artist),
                None => select_playlist(&playlists),
            };
            card_row(card, kind, circular, select)
        }))
        .into_any_element()
}

fn artwork(
    url: Option<SharedString>,
    size: f32,
    circular: bool,
    muted: Hsla,
    background: Hsla,
) -> AnyElement {
    div()
        .size(px(size))
        .flex_shrink_0()
        .when(circular, |this| this.rounded_full())
        .when(!circular, |this| this.rounded(px(4.)))
        .overflow_hidden()
        .flex()
        .items_center()
        .justify_center()
        .bg(background)
        .map(|this| match url {
            Some(url) => this.child(img(url).size(px(size)).object_fit(ObjectFit::Cover)),
            None => this.child(Icon::new(IconName::Music).size_4().text_color(muted)),
        })
        .into_any_element()
}

fn describe(kind: &str, subtitle: Option<SharedString>) -> SharedString {
    match subtitle {
        Some(subtitle) => SharedString::from(format!("{kind} • {subtitle}")),
        None => SharedString::from(kind.to_owned()),
    }
}

fn card_row(card: CardSummary, kind: &'static str, circular: bool, select: Select) -> AnyElement {
    let id = card.id.clone();
    let row_id = format!("search-row-{}", card.id);
    gpui_kit::div()
        .w_full()
        .child(
            Button::new(row_id)
                .cursor_pointer()
                .ghost()
                .w_full()
                .h(px(56.))
                .px_2()
                .child(RowContent {
                    thumbnail: card.thumbnail_url,
                    title: card.title,
                    subtitle: describe(kind, card.subtitle),
                    circular,
                })
                .on_click(move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                    select(id.clone(), cx)
                }),
        )
        .into_any_element()
}

#[derive(gpui_kit::IntoElement)]
struct RowContent {
    thumbnail: Option<SharedString>,
    title: SharedString,
    subtitle: SharedString,
    circular: bool,
}

impl gpui_kit::RenderOnce for RowContent {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let foreground = cx.theme().foreground;
        h_flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_x_3()
            .child(artwork(
                self.thumbnail,
                40.,
                self.circular,
                muted,
                cx.theme().muted,
            ))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(foreground)
                            .line_clamp(1)
                            .child(self.title),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .line_clamp(1)
                            .child(self.subtitle),
                    ),
            )
    }
}

fn render_top_lead(
    colors: (Hsla, Hsla),
    item: MixedItemSummary,
    playlists: &Entity<PlaylistsModel>,
    artist: &Entity<ArtistModel>,
    player: &Entity<PlayerModel>,
) -> AnyElement {
    let (card, kind, circular, select) = match item {
        MixedItemSummary::Artist(card) => (card, "Artist", true, select_artist(artist)),
        MixedItemSummary::Album(card) => (card, "Album", false, select_playlist(playlists)),
        MixedItemSummary::Playlist(card) => (card, "Playlist", false, select_playlist(playlists)),
        MixedItemSummary::Track(track) => {
            let card = CardSummary {
                id: track.id.clone(),
                title: track.title.clone(),
                subtitle: track.artist.clone(),
                thumbnail_url: track.thumbnail_url.clone(),
            };
            let player = player.clone();
            let queue = vec![track];
            return top_lead(
                colors,
                card,
                "Song",
                false,
                Box::new(move |_, cx| {
                    let queue = queue.clone();
                    player.update(cx, |model, cx| model.request_track_click(queue, 0, cx));
                }),
            );
        }
    };
    top_lead(colors, card, kind, circular, select)
}

fn top_lead(
    colors: (Hsla, Hsla),
    card: CardSummary,
    kind: &'static str,
    circular: bool,
    select: Select,
) -> AnyElement {
    let id = card.id.clone();
    Button::new(format!("search-top-{}", card.id))
        .cursor_pointer()
        .ghost()
        .h(px(128.))
        .w(px(380.))
        .flex_shrink_0()
        .child(
            h_flex()
                .w_full()
                .min_w_0()
                .items_center()
                .gap_x_4()
                .child(artwork(
                    card.thumbnail_url,
                    96.,
                    circular,
                    colors.0,
                    colors.1,
                ))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_1()
                        .child(
                            div()
                                .whitespace_normal()
                                .text_lg()
                                .font_weight(FontWeight::BOLD)
                                .line_clamp(2)
                                .child(card.title),
                        )
                        .child(
                            div()
                                .whitespace_normal()
                                .text_sm()
                                .line_clamp(2)
                                .child(describe(kind, card.subtitle)),
                        ),
                ),
        )
        .on_click(move |_: &ClickEvent, _: &mut Window, cx: &mut App| select(id.clone(), cx))
        .into_any_element()
}

fn empty_message(muted: Hsla, kind: SearchKind) -> AnyElement {
    div()
        .text_sm()
        .text_color(muted)
        .child(SharedString::from(format!(
            "No {} found.",
            kind.label().to_lowercase()
        )))
        .into_any_element()
}
