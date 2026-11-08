use encore_core::{Album, Artist, ArtistPage, Track};
use serde_json::Value;

use crate::identity::{artist_id, find_all, find_all_shallow, find_first, text, text_with_links};
use crate::playlists::thumbnail_url;

const HEADER_KEYS: [&str; 2] = ["musicImmersiveHeaderRenderer", "musicVisualHeaderRenderer"];
const SEPARATOR: &str = "•";

pub(crate) fn parse_artist_page(body: &str) -> Result<ArtistPage, serde_json::Error> {
    let root: Value = serde_json::from_str(body)?;
    let header = HEADER_KEYS
        .into_iter()
        .find_map(|key| find_all(&root, key).into_iter().next());

    let name = header.and_then(|header| text(header.get("title"))).unwrap_or_default();
    let description = header.and_then(|header| text_with_links(header.get("description")));
    let subscribers = header
        .and_then(|header| find_first(header, "subscriberCountText"))
        .and_then(|node| text(Some(node)));
    let header_image_url = header.and_then(thumbnail_url);

    let track_shelves = find_all_shallow(&root, "musicShelfRenderer");
    let tracks = track_shelves
        .iter()
        .flat_map(|shelf| find_all_shallow(shelf, "musicResponsiveListItemRenderer"))
        .filter_map(parse_song_row)
        .collect();
    let tracks_playlist_id = track_shelves
        .into_iter()
        .find_map(|shelf| {
            find_first(shelf, "bottomEndpoint").or_else(|| find_first(shelf, "bottomButton"))
        })
        .and_then(|endpoint| find_first(endpoint, "browseId"))
        .and_then(Value::as_str)
        .map(str::to_owned);

    let mut albums = Vec::new();
    let mut singles = Vec::new();
    let mut similar_artists = Vec::new();

    for carousel in find_all_shallow(&root, "musicCarouselShelfRenderer") {
        let title = carousel_title(carousel);
        let cards = find_all_shallow(carousel, "musicTwoRowItemRenderer");
        match title.as_deref() {
            Some("Albums") => albums.extend(cards.into_iter().filter_map(parse_album_card)),
            Some("Singles") | Some("Singles & EPs") => {
                singles.extend(cards.into_iter().filter_map(parse_album_card))
            }
            Some("Fans might also like") | Some("Similar artists") => {
                similar_artists.extend(cards.into_iter().filter_map(parse_artist_card))
            }
            _ => {}
        }
    }

    Ok(ArtistPage {
        name,
        description,
        subscribers,
        header_image_url,
        tracks,
        tracks_playlist_id,
        albums,
        singles,
        similar_artists,
    })
}

fn carousel_title(carousel: &Value) -> Option<String> {
    let header = carousel
        .get("header")?
        .get("musicCarouselShelfBasicHeaderRenderer")?;
    text(header.get("title"))
}

fn two_row_browse_id(item: &Value) -> Option<String> {
    find_all(item, "browseEndpoint")
        .into_iter()
        .find_map(|endpoint| endpoint.get("browseId")?.as_str())
        .map(str::to_owned)
}

fn parse_album_card(item: &Value) -> Option<Album> {
    let id = two_row_browse_id(item)?;
    let title = text(item.get("title"))?;

    let mut parts = segments(item.get("subtitle"));
    if parts
        .first()
        .is_some_and(|first| matches!(first.as_str(), "Album" | "Single" | "EP"))
    {
        parts.remove(0);
    }

    Some(Album {
        id,
        title,
        artist: None,
        year: parts.into_iter().next(),
        thumbnail_url: thumbnail_url(item),
    })
}

fn parse_artist_card(item: &Value) -> Option<Artist> {
    let id = two_row_browse_id(item)?;
    let name = text(item.get("title"))?;
    let subscribers = text(item.get("subtitle"));

    Some(Artist {
        id,
        name,
        subscribers,
        thumbnail_url: thumbnail_url(item),
    })
}

fn parse_song_row(item: &Value) -> Option<Track> {
    let flex = item.get("flexColumns")?.as_array()?;
    let title = text(column(flex, 0)?.get("text"))?;
    let id = find_all(item, "watchEndpoint")
        .into_iter()
        .find_map(|endpoint| endpoint.get("videoId")?.as_str())?
        .to_owned();

    let artist_id = column(flex, 1).and_then(|column| artist_id(column.get("text")));
    let mut parts = column(flex, 1)
        .map(|column| segments(column.get("text")))
        .unwrap_or_default();
    if parts
        .first()
        .is_some_and(|first| matches!(first.as_str(), "Song" | "Video"))
    {
        parts.remove(0);
    }
    let duration = match parts.last() {
        Some(last) if parts.len() > 1 && is_duration(last) => parts.pop(),
        _ => None,
    };
    let mut parts = parts.into_iter();
    let artist = parts.next();
    let album = parts.next();

    Some(Track {
        id,
        title,
        artist,
        artist_id,
        album,
        duration,
        thumbnail_url: thumbnail_url(item),
        credits: Vec::new(),
    })
}

fn column(flex: &[Value], index: usize) -> Option<&Value> {
    flex.get(index)?
        .get("musicResponsiveListItemFlexColumnRenderer")
}

fn segments(node: Option<&Value>) -> Vec<String> {
    let runs = match node
        .and_then(|node| node.get("runs"))
        .and_then(Value::as_array)
    {
        Some(runs) => runs,
        None => return Vec::new(),
    };
    let mut segments = Vec::new();
    let mut current = String::new();
    for run in runs {
        let run_text = run.get("text").and_then(Value::as_str).unwrap_or_default();
        match run_text.trim() == SEPARATOR {
            true => segments.push(std::mem::take(&mut current)),
            false => current.push_str(run_text),
        }
    }
    segments.push(current);
    segments
        .into_iter()
        .map(|segment| segment.trim().to_owned())
        .filter(|segment| !segment.is_empty())
        .collect()
}

fn is_duration(segment: &str) -> bool {
    let mut parts = segment.split(':');
    let count = parts
        .by_ref()
        .map(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
        .take_while(|valid| *valid)
        .count();
    (2..=3).contains(&count) && segment.split(':').count() == count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(title: &str, subscribers: &str, description: &str) -> String {
        format!(
            r#""musicImmersiveHeaderRenderer": {{
                "title": {{"runs": [{{"text": "{title}"}}]}},
                "description": {{"runs": [{{"text": "{description}"}}]}},
                "subscriptionButton": {{"subscribeButtonRenderer": {{
                    "subscriberCountText": {{"runs": [{{"text": "{subscribers}"}}]}}
                }}}},
                "thumbnail": {{"musicThumbnailRenderer": {{"thumbnail": {{"thumbnails": [
                    {{"url": "https://img.example/header.jpg"}}
                ]}}}}}}
            }}"#
        )
    }

    fn two_row_card(browse_id: &str, title: &str, subtitle_runs: &str) -> String {
        format!(
            r#"{{"musicTwoRowItemRenderer": {{
                "title": {{"runs": [{{"text": "{title}"}}]}},
                "subtitle": {{"runs": [{subtitle_runs}]}},
                "navigationEndpoint": {{"browseEndpoint": {{"browseId": "{browse_id}"}}}},
                "thumbnailRenderer": {{"musicThumbnailRenderer": {{"thumbnail": {{"thumbnails": [
                    {{"url": "https://img.example/{browse_id}.jpg"}}
                ]}}}}}}
            }}}}"#
        )
    }

    fn carousel(title: &str, cards: &str) -> String {
        format!(
            r#"{{"musicCarouselShelfRenderer": {{
                "header": {{"musicCarouselShelfBasicHeaderRenderer": {{
                    "title": {{"runs": [{{"text": "{title}"}}]}}
                }}}},
                "contents": [{cards}]
            }}}}"#
        )
    }

    fn body(header: &str, sections: &str) -> String {
        format!(
            r#"{{
                "header": {{{header}}},
                "contents": {{"singleColumnBrowseResultsRenderer": {{"tabs": [{{"tabRenderer": {{"content": {{"sectionListRenderer": {{"contents": [
                    {sections}
                ]}}}}}}}}]}}}}
            }}"#
        )
    }

    #[test]
    fn the_header_is_parsed() {
        let page = parse_artist_page(&body(
            &header("Kenshi Yonezu", "52.7M monthly audience", "A songwriter."),
            "",
        ))
        .unwrap();

        assert_eq!(page.name, "Kenshi Yonezu");
        assert_eq!(page.subscribers.as_deref(), Some("52.7M monthly audience"));
        assert_eq!(page.description.as_deref(), Some("A songwriter."));
        assert_eq!(
            page.header_image_url.as_deref(),
            Some("https://img.example/header.jpg")
        );
    }

    #[test]
    fn albums_and_singles_are_sorted_into_their_own_shelves() {
        let sections = format!(
            "{},{}",
            carousel(
                "Albums",
                &two_row_card(
                    "MPREb_a",
                    "STRAY SHEEP",
                    r#"{"text": "Album"}, {"text": " • "}, {"text": "2020"}"#,
                ),
            ),
            carousel(
                "Singles & EPs",
                &two_row_card(
                    "MPREb_b",
                    "KICK BACK",
                    r#"{"text": "Single"}, {"text": " • "}, {"text": "2022"}"#,
                ),
            ),
        );
        let page = parse_artist_page(&body(&header("X", "", ""), &sections)).unwrap();

        assert_eq!(
            page.albums,
            [Album {
                id: "MPREb_a".into(),
                title: "STRAY SHEEP".into(),
                artist: None,
                year: Some("2020".into()),
                thumbnail_url: Some("https://img.example/MPREb_a.jpg".into()),
            }]
        );
        assert_eq!(
            page.singles,
            [Album {
                id: "MPREb_b".into(),
                title: "KICK BACK".into(),
                artist: None,
                year: Some("2022".into()),
                thumbnail_url: Some("https://img.example/MPREb_b.jpg".into()),
            }]
        );
    }

    #[test]
    fn similar_artists_are_read_off_the_fans_might_also_like_shelf() {
        let sections = carousel(
            "Fans might also like",
            &two_row_card(
                "UCabc",
                "Hikaru Utada",
                r#"{"text": "20.1M monthly audience"}"#,
            ),
        );
        let page = parse_artist_page(&body(&header("X", "", ""), &sections)).unwrap();

        assert_eq!(
            page.similar_artists,
            [Artist {
                id: "UCabc".into(),
                name: "Hikaru Utada".into(),
                subscribers: Some("20.1M monthly audience".into()),
                thumbnail_url: Some("https://img.example/UCabc.jpg".into()),
            }]
        );
    }

    #[test]
    fn top_songs_are_read_from_the_music_shelf() {
        let rows = format!(
            r#"{{"musicResponsiveListItemRenderer": {{
                "flexColumns": [
                    {{"musicResponsiveListItemFlexColumnRenderer": {{"text": {{"runs": [
                        {{"text": "Lemon", "navigationEndpoint": {{"watchEndpoint": {{"videoId": "id1"}}}}}}
                    ]}}}}}},
                    {{"musicResponsiveListItemFlexColumnRenderer": {{"text": {{"runs": [
                        {{"text": "Kenshi Yonezu"}}
                    ]}}}}}}
                ]
            }}}}"#
        );
        let sections = format!(
            r#"{{"musicShelfRenderer": {{"contents": [{rows}]}}}}"#
        );
        let page = parse_artist_page(&body(&header("X", "", ""), &sections)).unwrap();

        assert_eq!(page.tracks.len(), 1);
        assert_eq!(page.tracks[0].id, "id1");
        assert_eq!(page.tracks[0].title, "Lemon");
        assert_eq!(page.tracks_playlist_id, None);
    }

    #[test]
    fn the_top_songs_show_all_button_is_the_tracks_playlist_id_too() {
        let sections = r#"{"musicShelfRenderer": {
            "contents": [],
            "bottomButton": {"buttonRenderer": {"navigationEndpoint": {"browseEndpoint": {"browseId": "VLPLshowall"}}}}
        }}"#;
        let page = parse_artist_page(&body(&header("X", "", ""), sections)).unwrap();

        assert_eq!(page.tracks_playlist_id.as_deref(), Some("VLPLshowall"));
    }

    #[test]
    fn the_top_songs_shelfs_more_button_is_the_tracks_playlist_id() {
        let sections = r#"{"musicShelfRenderer": {
            "contents": [],
            "bottomEndpoint": {"browseEndpoint": {"browseId": "VLPLplaylist123"}}
        }}"#;
        let page = parse_artist_page(&body(&header("X", "", ""), sections)).unwrap();

        assert_eq!(page.tracks_playlist_id.as_deref(), Some("VLPLplaylist123"));
    }

    #[test]
    fn something_that_is_not_json_is_an_error() {
        assert!(parse_artist_page("<html>").is_err());
    }
}
