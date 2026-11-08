use encore_core::{Album, Artist, MixedItem, MixedResults, Playlist, Track, TopResult};
use serde_json::Value;

use crate::identity::{artist_id, find_all, find_all_shallow, text};
use crate::playlists::{thumbnail_url, track_count};

pub(crate) const SONGS_FILTER: &str = "EgWKAQIIAWoMEA4QChADEAQQCRAF";
pub(crate) const VIDEOS_FILTER: &str = "EgWKAQIQAWoQEAMQBBAJEAoQBRAREBAQFQ==";
pub(crate) const ALBUMS_FILTER: &str = "EgWKAQIYAWoQEAMQBBAJEAoQBRAREBAQFQ==";
pub(crate) const ARTISTS_FILTER: &str = "EgWKAQIgAWoQEAMQBBAJEAoQBRAREBAQFQ==";
pub(crate) const PLAYLISTS_FILTER: &str = "EgeKAQQoAEABahAQAxAEEAkQChAFEBEQEBAV";

const SEPARATOR: &str = "•";

pub(crate) fn parse_song_search(body: &str) -> Result<Vec<Track>, serde_json::Error> {
    let root: Value = serde_json::from_str(body)?;
    Ok(find_all_shallow(&root, "musicResponsiveListItemRenderer")
        .into_iter()
        .filter_map(parse_song_row)
        .collect())
}

pub(crate) fn parse_song_row(item: &Value) -> Option<Track> {
    let flex = item.get("flexColumns")?.as_array()?;
    let title = text(column(flex, 0)?.get("text"))?;
    let id = find_all(item, "watchEndpoint")
        .into_iter()
        .find_map(|endpoint| endpoint.get("videoId")?.as_str())?
        .to_owned();

    let artist_id = column(flex, 1).and_then(|column| artist_id(column.get("text")));
    let mut segments = column(flex, 1)
        .map(|column| segments(column.get("text")))
        .unwrap_or_default();
    if segments
        .first()
        .is_some_and(|first| matches!(first.as_str(), "Song" | "Video"))
    {
        segments.remove(0);
    }
    let duration = match segments.last() {
        Some(last) if segments.len() > 1 && is_duration(last) => segments.pop(),
        _ => None,
    };
    let mut segments = segments.into_iter();
    let artist = segments.next();
    let album = segments.next();

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

pub(crate) fn parse_album_search(body: &str) -> Result<Vec<Album>, serde_json::Error> {
    let root: Value = serde_json::from_str(body)?;
    Ok(find_all_shallow(&root, "musicResponsiveListItemRenderer")
        .into_iter()
        .filter_map(parse_album_row)
        .collect())
}

fn parse_album_row(item: &Value) -> Option<Album> {
    let flex = item.get("flexColumns")?.as_array()?;
    let title = text(column(flex, 0)?.get("text"))?;
    let id = browse_id(item)?;

    let mut segments = column(flex, 1)
        .map(|column| segments(column.get("text")))
        .unwrap_or_default();
    if segments.first().is_some_and(|first| first == "Album") {
        segments.remove(0);
    }
    let mut segments = segments.into_iter();

    Some(Album {
        id,
        title,
        artist: segments.next(),
        year: segments.next(),
        thumbnail_url: thumbnail_url(item),
    })
}

pub(crate) fn parse_artist_search(body: &str) -> Result<Vec<Artist>, serde_json::Error> {
    let root: Value = serde_json::from_str(body)?;
    Ok(find_all_shallow(&root, "musicResponsiveListItemRenderer")
        .into_iter()
        .filter_map(parse_artist_row)
        .collect())
}

fn parse_artist_row(item: &Value) -> Option<Artist> {
    let flex = item.get("flexColumns")?.as_array()?;
    let name = text(column(flex, 0)?.get("text"))?;
    let id = browse_id(item)?;

    let mut segments = column(flex, 1)
        .map(|column| segments(column.get("text")))
        .unwrap_or_default();
    if segments.first().is_some_and(|first| first == "Artist") {
        segments.remove(0);
    }

    Some(Artist {
        id,
        name,
        subscribers: segments.into_iter().next(),
        thumbnail_url: thumbnail_url(item),
    })
}

pub(crate) fn parse_playlist_search(body: &str) -> Result<Vec<Playlist>, serde_json::Error> {
    let root: Value = serde_json::from_str(body)?;
    Ok(find_all_shallow(&root, "musicResponsiveListItemRenderer")
        .into_iter()
        .filter_map(parse_playlist_row)
        .collect())
}

fn parse_playlist_row(item: &Value) -> Option<Playlist> {
    let flex = item.get("flexColumns")?.as_array()?;
    let title = text(column(flex, 0)?.get("text"))?;
    let id = browse_id(item)?;
    let subtitle = column(flex, 1).and_then(|column| text(column.get("text")));

    Some(Playlist {
        id,
        title,
        track_count: subtitle.as_deref().and_then(track_count),
        thumbnail_url: thumbnail_url(item),
        owner: None,
    })
}

pub(crate) fn parse_mixed_search(body: &str) -> Result<MixedResults, serde_json::Error> {
    let root: Value = serde_json::from_str(body)?;

    let top_result = find_all_shallow(&root, "musicCardShelfRenderer")
        .into_iter()
        .next()
        .and_then(parse_top_result);

    let items = find_all_shallow(&root, "musicShelfRenderer")
        .into_iter()
        .flat_map(|shelf| find_all_shallow(shelf, "musicResponsiveListItemRenderer"))
        .chain(
            find_all_shallow(&root, "itemSectionRenderer")
                .into_iter()
                .flat_map(|section| find_all_shallow(section, "musicResponsiveListItemRenderer")),
        )
        .filter_map(classify_row)
        .collect();

    Ok(MixedResults { top_result, items })
}

fn classify_row(item: &Value) -> Option<MixedItem> {
    let flex = item.get("flexColumns")?.as_array()?;
    let label = column(flex, 1)
        .map(|column| segments(column.get("text")))
        .unwrap_or_default()
        .into_iter()
        .next();

    match label.as_deref() {
        Some("Artist") => parse_artist_row(item).map(MixedItem::Artist),
        Some("Playlist") | Some("Community playlist") => {
            parse_playlist_row(item).map(MixedItem::Playlist)
        }
        Some("Album") | Some("Single") | Some("EP") => parse_album_row(item).map(MixedItem::Album),
        _ => parse_song_row(item).map(MixedItem::Track),
    }
}

fn parse_top_result(card: &Value) -> Option<TopResult> {
    let title = text(card.get("title"))?;
    let on_tap = card.get("onTap")?;
    let browse_id = find_all(on_tap, "browseEndpoint")
        .into_iter()
        .find_map(|endpoint| endpoint.get("browseId")?.as_str())
        .map(str::to_owned);
    let video_id = find_all(on_tap, "watchEndpoint")
        .into_iter()
        .find_map(|endpoint| endpoint.get("videoId")?.as_str())
        .map(str::to_owned);

    let mut parts = segments(card.get("subtitle"));
    let label = (!parts.is_empty()).then(|| parts.remove(0));
    let thumbnail = card.get("thumbnail").and_then(thumbnail_url);

    let item = match label.as_deref() {
        Some("Artist") => MixedItem::Artist(Artist {
            id: browse_id?,
            name: title,
            subscribers: parts.into_iter().next(),
            thumbnail_url: thumbnail,
        }),
        Some("Album") | Some("Single") | Some("EP") => MixedItem::Album(Album {
            id: browse_id?,
            title,
            artist: parts.first().cloned(),
            year: parts.get(1).cloned(),
            thumbnail_url: thumbnail,
        }),
        Some("Playlist") | Some("Community playlist") => MixedItem::Playlist(Playlist {
            id: browse_id?,
            title,
            track_count: parts.iter().find_map(|segment| track_count(segment)),
            thumbnail_url: thumbnail,
            owner: None,
        }),
        _ => MixedItem::Track(Track {
            id: video_id.or(browse_id)?,
            title,
            artist: parts.into_iter().next(),
            artist_id: None,
            album: None,
            duration: None,
            thumbnail_url: thumbnail,
            credits: Vec::new(),
        }),
    };

    let quick_picks = find_all_shallow(card, "musicResponsiveListItemRenderer")
        .into_iter()
        .filter_map(parse_song_row)
        .collect();

    Some(TopResult { item, quick_picks })
}

fn browse_id(item: &Value) -> Option<String> {
    find_all(item, "browseEndpoint")
        .into_iter()
        .find_map(|endpoint| endpoint.get("browseId")?.as_str())
        .map(str::to_owned)
}

fn column(flex: &[Value], index: usize) -> Option<&Value> {
    flex.get(index)?
        .get("musicResponsiveListItemFlexColumnRenderer")
}

pub(crate) fn segments(node: Option<&Value>) -> Vec<String> {
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

    fn row(video_id: &str, title: &str, subtitle_runs: &str) -> String {
        format!(
            r#"{{"musicResponsiveListItemRenderer": {{
                "thumbnail": {{"musicThumbnailRenderer": {{"thumbnail": {{"thumbnails": [
                    {{"url": "https://img.example/{video_id}.jpg"}}
                ]}}}}}},
                "flexColumns": [
                    {{"musicResponsiveListItemFlexColumnRenderer": {{"text": {{"runs": [
                        {{"text": "{title}", "navigationEndpoint": {{"watchEndpoint": {{"videoId": "{video_id}"}}}}}}
                    ]}}}}}},
                    {{"musicResponsiveListItemFlexColumnRenderer": {{"text": {{"runs": [{subtitle_runs}]}}}}}}
                ]
            }}}}"#
        )
    }

    fn body(rows: &str) -> String {
        format!(
            r#"{{"contents": {{"tabbedSearchResultsRenderer": {{"tabs": [{{"tabRenderer": {{"content": {{"sectionListRenderer": {{"contents": [
                {{"musicShelfRenderer": {{"contents": [{rows}]}}}}
            ]}}}}}}}}]}}}}}}"#
        )
    }

    #[test]
    fn artist_album_and_duration_are_read_off_the_subtitle_segments() {
        let rows = row(
            "id1",
            "Yodaka",
            r#"{"text": "Kenshi Yonezu"}, {"text": " • "}, {"text": "STRAY SHEEP"}, {"text": " • "}, {"text": "4:12"}"#,
        );

        let tracks = parse_song_search(&body(&rows)).unwrap();

        assert_eq!(
            tracks,
            [Track {
                id: "id1".into(),
                title: "Yodaka".into(),
                artist: Some("Kenshi Yonezu".into()),
                artist_id: None,
                album: Some("STRAY SHEEP".into()),
                duration: Some("4:12".into()),
                thumbnail_url: Some("https://img.example/id1.jpg".into()),
                credits: Vec::new(),
            }]
        );
    }

    #[test]
    fn several_artist_runs_are_joined_into_one_segment() {
        let rows = row(
            "id1",
            "Duet",
            r#"{"text": "A"}, {"text": " & "}, {"text": "B"}, {"text": " • "}, {"text": "3:00"}"#,
        );

        let tracks = parse_song_search(&body(&rows)).unwrap();

        assert_eq!(tracks[0].artist.as_deref(), Some("A & B"));
        assert_eq!(tracks[0].album, None);
        assert_eq!(tracks[0].duration.as_deref(), Some("3:00"));
    }

    #[test]
    fn a_leading_song_label_is_dropped() {
        let rows = row(
            "id1",
            "Title",
            r#"{"text": "Song"}, {"text": " • "}, {"text": "Artist"}, {"text": " • "}, {"text": "2:05"}"#,
        );

        let tracks = parse_song_search(&body(&rows)).unwrap();

        assert_eq!(tracks[0].artist.as_deref(), Some("Artist"));
    }

    #[test]
    fn a_row_without_a_video_id_is_skipped() {
        let rows = r#"{"musicResponsiveListItemRenderer": {"flexColumns": [
            {"musicResponsiveListItemFlexColumnRenderer": {"text": {"runs": [{"text": "Album"}]}}}
        ]}}"#;

        assert_eq!(parse_song_search(&body(rows)).unwrap(), []);
    }

    #[test]
    fn something_that_is_not_json_is_an_error() {
        assert!(parse_song_search("<html>").is_err());
    }

    fn browse_row(browse_id: &str, title: &str, subtitle_runs: &str) -> String {
        format!(
            r#"{{"musicResponsiveListItemRenderer": {{
                "thumbnail": {{"musicThumbnailRenderer": {{"thumbnail": {{"thumbnails": [
                    {{"url": "https://img.example/{browse_id}.jpg"}}
                ]}}}}}},
                "flexColumns": [
                    {{"musicResponsiveListItemFlexColumnRenderer": {{"text": {{"runs": [
                        {{"text": "{title}", "navigationEndpoint": {{"browseEndpoint": {{"browseId": "{browse_id}"}}}}}}
                    ]}}}}}},
                    {{"musicResponsiveListItemFlexColumnRenderer": {{"text": {{"runs": [{subtitle_runs}]}}}}}}
                ]
            }}}}"#
        )
    }

    #[test]
    fn an_album_row_is_parsed_with_artist_and_year() {
        let rows = browse_row(
            "MPREb_abc",
            "STRAY SHEEP",
            r#"{"text": "Album"}, {"text": " • "}, {"text": "Kenshi Yonezu"}, {"text": " • "}, {"text": "2020"}"#,
        );

        let albums = parse_album_search(&body(&rows)).unwrap();

        assert_eq!(
            albums,
            [Album {
                id: "MPREb_abc".into(),
                title: "STRAY SHEEP".into(),
                artist: Some("Kenshi Yonezu".into()),
                year: Some("2020".into()),
                thumbnail_url: Some("https://img.example/MPREb_abc.jpg".into()),
            }]
        );
    }

    #[test]
    fn an_artist_row_is_parsed_with_subscribers() {
        let rows = browse_row(
            "UCabc",
            "Kenshi Yonezu",
            r#"{"text": "Artist"}, {"text": " • "}, {"text": "52.7M subscribers"}"#,
        );

        let artists = parse_artist_search(&body(&rows)).unwrap();

        assert_eq!(
            artists,
            [Artist {
                id: "UCabc".into(),
                name: "Kenshi Yonezu".into(),
                subscribers: Some("52.7M subscribers".into()),
                thumbnail_url: Some("https://img.example/UCabc.jpg".into()),
            }]
        );
    }

    #[test]
    fn a_playlist_row_is_parsed_with_a_song_count() {
        let rows = browse_row(
            "VLabc",
            "Presenting Kenshi Yonezu",
            r#"{"text": "Playlist"}, {"text": " • "}, {"text": "YouTube Music"}, {"text": " • "}, {"text": "45 songs"}"#,
        );

        let playlists = parse_playlist_search(&body(&rows)).unwrap();

        assert_eq!(
            playlists,
            [Playlist {
                id: "VLabc".into(),
                title: "Presenting Kenshi Yonezu".into(),
                track_count: Some(45),
                thumbnail_url: Some("https://img.example/VLabc.jpg".into()),
                owner: None,
            }]
        );
    }

    #[test]
    fn a_browse_row_without_a_browse_id_is_skipped() {
        let body = body(r#"{"musicResponsiveListItemRenderer": {"flexColumns": [
            {"musicResponsiveListItemFlexColumnRenderer": {"text": {"runs": [{"text": "Untitled"}]}}}
        ]}}"#);

        assert_eq!(parse_album_search(&body).unwrap(), []);
        assert_eq!(parse_artist_search(&body).unwrap(), []);
        assert_eq!(parse_playlist_search(&body).unwrap(), []);
    }

    fn card_shelf(title: &str, subtitle_runs: &str, browse_id: &str, quick_picks: &str) -> String {
        format!(
            r#"{{"musicCardShelfRenderer": {{
                "title": {{"runs": [{{"text": "{title}"}}]}},
                "subtitle": {{"runs": [{subtitle_runs}]}},
                "onTap": {{"browseEndpoint": {{"browseId": "{browse_id}"}}}},
                "thumbnail": {{"musicThumbnailRenderer": {{"thumbnail": {{"thumbnails": [
                    {{"url": "https://img.example/{browse_id}.jpg"}}
                ]}}}}}},
                "contents": [{quick_picks}]
            }}}}"#
        )
    }

    fn mixed_body(sections: &str) -> String {
        format!(
            r#"{{"contents": {{"tabbedSearchResultsRenderer": {{"tabs": [{{"tabRenderer": {{"content": {{"sectionListRenderer": {{"contents": [
                {sections}
            ]}}}}}}}}]}}}}}}"#
        )
    }

    #[test]
    fn the_top_result_card_is_parsed_as_an_artist_with_its_quick_picks() {
        let quick_pick = row("id1", "Lemon", r#"{"text": "Kenshi Yonezu"}"#);
        let card = card_shelf(
            "Kenshi Yonezu",
            r#"{"text": "Artist"}, {"text": " • "}, {"text": "52.7M monthly audience"}"#,
            "UCabc",
            &quick_pick,
        );

        let results = parse_mixed_search(&mixed_body(&card)).unwrap();
        let top_result = results.top_result.unwrap();

        assert_eq!(
            top_result.item,
            MixedItem::Artist(Artist {
                id: "UCabc".into(),
                name: "Kenshi Yonezu".into(),
                subscribers: Some("52.7M monthly audience".into()),
                thumbnail_url: Some("https://img.example/UCabc.jpg".into()),
            })
        );
        assert_eq!(top_result.quick_picks.len(), 1);
        assert_eq!(top_result.quick_picks[0].title, "Lemon");
    }

    #[test]
    fn mixed_rows_are_classified_by_their_subtitle_label() {
        let song_row = row(
            "id1",
            "Yodaka",
            r#"{"text": "Song"}, {"text": " • "}, {"text": "Kenshi Yonezu"}, {"text": " • "}, {"text": "4:12"}"#,
        );
        let artist_row = browse_row(
            "UCabc",
            "Kenshi Yonezu",
            r#"{"text": "Artist"}, {"text": " • "}, {"text": "52.7M subscribers"}"#,
        );
        let album_row = browse_row(
            "MPREb_abc",
            "STRAY SHEEP",
            r#"{"text": "Album"}, {"text": " • "}, {"text": "2020"}"#,
        );
        let playlist_row = browse_row(
            "VLabc",
            "Presenting Kenshi Yonezu",
            r#"{"text": "Playlist"}, {"text": " • "}, {"text": "YouTube Music"}"#,
        );
        let shelf = format!(
            r#"{{"musicShelfRenderer": {{"contents": [{song_row}, {artist_row}, {album_row}, {playlist_row}]}}}}"#
        );

        let results = parse_mixed_search(&mixed_body(&shelf)).unwrap();

        assert_eq!(results.items.len(), 4);
        assert!(matches!(results.items[0], MixedItem::Track(_)));
        assert!(matches!(results.items[1], MixedItem::Artist(_)));
        assert!(matches!(results.items[2], MixedItem::Album(_)));
        assert!(matches!(results.items[3], MixedItem::Playlist(_)));
    }

    #[test]
    fn a_response_without_a_top_result_card_has_none() {
        let song_row = row("id1", "Yodaka", r#"{"text": "4:12"}"#);
        let shelf = format!(r#"{{"musicShelfRenderer": {{"contents": [{song_row}]}}}}"#);

        let results = parse_mixed_search(&mixed_body(&shelf)).unwrap();

        assert!(results.top_result.is_none());
        assert_eq!(results.items.len(), 1);
    }

    #[test]
    fn rows_wrapped_one_per_item_section_are_read_too() {
        let album_row = browse_row(
            "MPREb_abc",
            "STRAY SHEEP",
            r#"{"text": "Album"}, {"text": " • "}, {"text": "2020"}"#,
        );
        let song_row = row(
            "id1",
            "Yodaka",
            r#"{"text": "Song"}, {"text": " • "}, {"text": "Kenshi Yonezu"}"#,
        );
        let sections = format!(
            r#"{{"itemSectionRenderer": {{"contents": [{album_row}]}}}},
               {{"itemSectionRenderer": {{"contents": [{song_row}]}}}}"#
        );

        let results = parse_mixed_search(&mixed_body(&sections)).unwrap();

        assert_eq!(results.items.len(), 2);
        assert!(matches!(results.items[0], MixedItem::Album(_)));
        assert!(matches!(results.items[1], MixedItem::Track(_)));
    }
}
