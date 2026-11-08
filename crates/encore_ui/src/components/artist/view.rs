use gpui_kit::{
    ClickEvent, HighlightStyle, InteractiveText, StyledText, UnderlineStyle, px, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, Window,
    base::{h_flex, v_flex},
    component::{
        ActiveTheme as _,
        button::{Button, ButtonRounded, ButtonVariants as _},
    },
    div,
    prelude::FluentBuilder as _,
};

use crate::artist::{ArtistModel, CardSummary};
use crate::components::card::Card;
use crate::components::playlist::detail::{play_button, render_track, shuffle_button};
use crate::components::playlist::header::PlaylistHeader;
use crate::player::PlayerModel;
use crate::playlists::PlaylistsModel;

const DESCRIPTION_CLAMP_LINES: usize = 2;

const DESCRIPTION_COLLAPSIBLE_CHARS: usize = 160;

pub struct ArtistView {
    model: Entity<ArtistModel>,
    player: Entity<PlayerModel>,
    playlists: Entity<PlaylistsModel>,
    description_expanded: bool,
    _observe_model: Subscription,
    _observe_player: Subscription,
}

impl ArtistView {
    pub fn new(
        model: Entity<ArtistModel>,
        player: Entity<PlayerModel>,
        playlists: Entity<PlaylistsModel>,
        cx: &mut Context<Self>,
    ) -> Self {
        let observe_model = cx.observe(&model, |this: &mut Self, _, cx| {
            this.description_expanded = false;
            cx.notify();
        });
        let observe_player = cx.observe(&player, |_, _, cx| cx.notify());
        Self {
            model,
            player,
            playlists,
            description_expanded: false,
            _observe_model: observe_model,
            _observe_player: observe_player,
        }
    }
}

impl Render for ArtistView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let foreground = cx.theme().foreground;
        let on_primary = cx.theme().primary_foreground;
        let page = self.model.read(cx).page().cloned();
        let player_state = self.player.read(cx);
        let now_playing = player_state.current_track().map(|track| track.id.clone());
        let status = player_state.status();

        let Some(page) = page else {
            return div()
                .w_full()
                .min_w_0()
                .p_4()
                .text_sm()
                .text_color(muted)
                .child("Loading artist…")
                .into_any_element();
        };

        let meta_line = match page.subscribers {
            Some(subscribers) => SharedString::from(format!("Artist • {subscribers}")),
            None => SharedString::from("Artist"),
        };
        let mut header = PlaylistHeader::new(page.name)
            .subtitle(meta_line)
            .circular();
        if let Some(url) = page.header_image_url {
            header = header.poster(url);
        }

        let link_color = cx.theme().link;
        let description_expanded = self.description_expanded;
        let description = page.description.map(|description| {
            let collapsible =
                description.chars().count() > DESCRIPTION_COLLAPSIBLE_CHARS || description.contains('\n');
            let toggle = div()
                .id("artist-description-toggle")
                .text_sm()
                .font_weight(gpui_kit::FontWeight::MEDIUM)
                .text_color(cx.theme().link)
                .cursor_pointer()
                .child(match description_expanded {
                    true => "Less",
                    false => "More",
                })
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.description_expanded = !this.description_expanded;
                    cx.notify();
                }));

            v_flex()
                .w_full()
                .min_w_0()
                .gap_1()
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .text_sm()
                        .text_color(muted)
                        .whitespace_normal()
                        .when(collapsible && !description_expanded, |this| {
                            this.text_ellipsis().line_clamp(DESCRIPTION_CLAMP_LINES)
                        })
                        .child(linked_text(description, link_color)),
                )
                .when(collapsible, |this| this.child(toggle))
        });

        let actions = h_flex()
            .items_center()
            .gap_2()
            .child(play_button(page.tracks.clone(), self.player.clone(), on_primary))
            .child(shuffle_button(page.tracks.clone(), self.player.clone(), muted));

        let tracks =
            v_flex()
                .w_full()
                .min_w_0()
                .gap_1()
                .children(page.tracks.iter().enumerate().map(|(index, track)| {
                    let is_current = now_playing.as_ref() == Some(&track.id);
                    render_track(
                        index,
                        track.clone(),
                        is_current,
                        is_current && status.is_loading(),
                        is_current && status.is_playing(),
                        page.tracks.clone(),
                        &self.player,
                    )
                }));

        let playlists = self.playlists.clone();
        let show_all_songs = page.tracks_playlist_id.map(|playlist_id| {
            let playlists = playlists.clone();
            div().child(
                Button::new("artist-show-all-songs")
                    .cursor_pointer()
                    .label("Show all")
                    .compact()
                    .rounded(ButtonRounded::Large)
                    .secondary()
                    .on_click(
                        move |_: &ClickEvent, _: &mut Window, cx: &mut gpui_kit::App| {
                            let playlist_id = playlist_id.clone();
                            playlists.update(cx, |model, cx| model.select(playlist_id, cx));
                        },
                    ),
            )
        });
        let albums = card_shelf("Albums", page.albums, false, foreground, {
            let playlists = playlists.clone();
            move |id, cx| playlists.update(cx, |model, cx| model.select(id, cx))
        });
        let singles = card_shelf("Singles & EPs", page.singles, false, foreground, {
            let playlists = playlists.clone();
            move |id, cx| playlists.update(cx, |model, cx| model.select(id, cx))
        });
        let similar = card_shelf(
            "Fans might also like",
            page.similar_artists,
            true,
            foreground,
            {
                let model = self.model.clone();
                move |id, cx| model.update(cx, |model, cx| model.select(id, cx))
            },
        );

        let hero = v_flex()
            .w_full()
            .min_w_0()
            .p_5()
            .gap_4()
            .rounded(cx.theme().radius_lg)
            .border_1()
            .border_color(cx.theme().border)
            .child(header)
            .children(description)
            .child(actions);

        v_flex()
            .w_full()
            .min_w_0()
            .p_6()
            .gap_6()
            .child(hero)
            .when(!page.tracks.is_empty(), |this| {
                this.child(
                    v_flex()
                        .w_full()
                        .min_w_0()
                        .gap_3()
                        .child(
                            div()
                                .text_lg()
                                .font_weight(gpui_kit::FontWeight::BOLD)
                                .text_color(foreground)
                                .child("Top songs"),
                        )
                        .child(tracks)
                        .children(show_all_songs),
                )
            })
            .children(albums)
            .children(singles)
            .children(similar)
            .into_any_element()
    }
}

fn render_card(card: CardSummary, rounded_full: bool) -> Card {
    let item = Card::new(format!("artist-card-{}", card.id), card.title);
    let item = match card.subtitle {
        Some(subtitle) => item.subtitle(subtitle),
        None => item,
    };
    let item = match card.thumbnail_url {
        Some(url) => item.thumbnail(url),
        None => item,
    };
    match rounded_full {
        true => item.rounded_full(),
        false => item,
    }
}

fn card_shelf(
    title: &'static str,
    cards: Vec<CardSummary>,
    rounded_full: bool,
    foreground: gpui_kit::Hsla,
    on_click: impl Fn(SharedString, &mut gpui_kit::App) + 'static,
) -> Option<impl IntoElement> {
    if cards.is_empty() {
        return None;
    }
    let on_click = std::rc::Rc::new(on_click);
    Some(
        v_flex()
            .w_full()
            .min_w_0()
            .gap_3()
            .child(
                div()
                    .text_lg()
                    .font_weight(gpui_kit::FontWeight::BOLD)
                    .text_color(foreground)
                    .child(title),
            )
            .child(h_flex().w_full().min_w_0().flex_wrap().gap_4().children(
                cards.into_iter().map(|card| {
                    let id = card.id.clone();
                    let on_click = on_click.clone();
                    render_card(card, rounded_full).on_click(
                        move |_: &ClickEvent, _: &mut Window, cx: &mut gpui_kit::App| {
                            on_click(id.clone(), cx);
                        },
                    )
                }),
            )),
    )
}

fn link_ranges(text: &str) -> Vec<std::ops::Range<usize>> {
    let mut ranges = Vec::new();
    let mut offset = 0;
    for token in text.split_inclusive(char::is_whitespace) {
        let trimmed = token.trim_end();
        if let Some(start) = trimmed.find("http://").or_else(|| trimmed.find("https://")) {
            let candidate = &trimmed[start..];
            let end = candidate
                .trim_end_matches(|c: char| matches!(c, ')' | ']' | '.' | ',' | ';' | '!' | '?' | '"' | '\''))
                .len();
            if end > "https://".len() {
                ranges.push(offset + start..offset + start + end);
            }
        }
        offset += token.len();
    }
    ranges
}

fn linked_text(text: SharedString, link_color: gpui_kit::Hsla) -> InteractiveText {
    let ranges = link_ranges(&text);
    let style = HighlightStyle {
        color: Some(link_color),
        underline: Some(UnderlineStyle {
            thickness: px(1.),
            color: Some(link_color),
            wavy: false,
        }),
        ..Default::default()
    };
    let urls: Vec<String> = ranges.iter().map(|range| text[range.clone()].to_owned()).collect();
    let highlights = ranges.iter().cloned().map(|range| (range, style));
    InteractiveText::new(
        "artist-description-text",
        StyledText::new(text).with_highlights(highlights),
    )
    .on_click(ranges, move |index, _, cx| cx.open_url(&urls[index]))
}
