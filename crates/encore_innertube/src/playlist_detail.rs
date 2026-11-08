use encore_core::{PlaylistDetail, PlaylistPrivacy, Track};
use serde_json::Value;

use crate::identity::{find_all, find_all_shallow, text};
use crate::playlists::{thumbnail_url, track_count};

const HEADER_KEYS: [&str; 2] = ["musicResponsiveHeaderRenderer", "musicDetailHeaderRenderer"];

pub(crate) fn parse_playlist_detail(body: &str) -> Result<PlaylistDetail, serde_json::Error> {
    let root: Value = serde_json::from_str(body)?;
    let header = playlist_header(&root);

    let title = header
        .and_then(|header| text(header.get("title")))
        .unwrap_or_default();
    let second_subtitle = header.and_then(|header| text(header.get("secondSubtitle")));
    let first_subtitle = header.and_then(|header| text(header.get("subtitle")));
    let count_subtitle = second_subtitle.clone().or_else(|| first_subtitle.clone());
    let thumbnail = header.and_then(thumbnail_url);

    let tracks: Vec<Track> = find_all_shallow(&root, "musicResponsiveListItemRenderer")
        .into_iter()
        .filter_map(parse_track_row)
        .collect();

    let track_count = match count_subtitle.as_deref().and_then(track_count) {
        Some(count) => Some(count),
        None => u32::try_from(tracks.len()).ok().filter(|&count| count > 0),
    };

    Ok(PlaylistDetail {
        title,
        description: description(&root, header),
        thumbnail_url: thumbnail,
        track_count,
        year: first_subtitle.as_deref().and_then(trailing_segment).filter(is_year),
        total_duration: second_subtitle.as_deref().and_then(trailing_segment),
        privacy: playlist_privacy(&root),
        tracks,
    })
}

fn playlist_header(root: &Value) -> Option<&Value> {
    HEADER_KEYS
        .into_iter()
        .find_map(|key| find_all(root, key).into_iter().next())
}

fn trailing_segment(subtitle: &str) -> Option<String> {
    let (_, segment) = subtitle.rsplit_once(" • ")?;
    let segment = segment.trim();
    (!segment.is_empty()).then(|| segment.to_owned())
}

fn is_year(segment: &String) -> bool {
    segment.len() == 4 && segment.bytes().all(|b| b.is_ascii_digit())
}

fn playlist_privacy(root: &Value) -> Option<PlaylistPrivacy> {
    find_all(root, "musicPlaylistEditHeaderRenderer")
        .into_iter()
        .find_map(|header| header.get("privacy")?.as_str())
        .and_then(|privacy| match privacy {
            "PUBLIC" => Some(PlaylistPrivacy::Public),
            "PRIVATE" => Some(PlaylistPrivacy::Private),
            "UNLISTED" => Some(PlaylistPrivacy::Unlisted),
            _ => None,
        })
}

fn description(root: &Value, header: Option<&Value>) -> Option<String> {
    match header.and_then(|header| text(header.get("description"))) {
        Some(description) => Some(description),
        None => find_all(root, "musicDescriptionShelfRenderer")
            .into_iter()
            .find_map(|shelf| text(shelf.get("description"))),
    }
}

fn parse_track_row(item: &Value) -> Option<Track> {
    let flex = item.get("flexColumns")?.as_array()?;
    let title_text = flex
        .first()?
        .get("musicResponsiveListItemFlexColumnRenderer")?
        .get("text")?;
    let title = text(Some(title_text))?;
    let id = title_text
        .get("runs")?
        .as_array()?
        .first()?
        .get("navigationEndpoint")?
        .get("watchEndpoint")?
        .get("videoId")?
        .as_str()?
        .to_owned();

    let artist_column = flex
        .get(1)
        .and_then(|column| column.get("musicResponsiveListItemFlexColumnRenderer"))
        .and_then(|column| column.get("text"));
    let artist = artist_column.and_then(|column| text(Some(column)));
    let artist_id = crate::identity::artist_id(artist_column);

    let album = flex
        .get(2)
        .and_then(|column| column.get("musicResponsiveListItemFlexColumnRenderer"))
        .and_then(|column| text(column.get("text")));

    let duration = item
        .get("fixedColumns")
        .and_then(Value::as_array)
        .and_then(|columns| columns.first())
        .and_then(|column| column.get("musicResponsiveListItemFixedColumnRenderer"))
        .and_then(|column| text(column.get("text")));

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

#[cfg(test)]
mod tests {
    use super::*;

    fn track_row(title: &str, video_id: &str, artist: &str, duration: &str) -> String {
        format!(
            r#"{{"musicResponsiveListItemRenderer": {{
                "flexColumns": [
                    {{"musicResponsiveListItemFlexColumnRenderer": {{"text": {{"runs": [
                        {{"navigationEndpoint": {{"watchEndpoint": {{"videoId": "{video_id}"}}}}, "text": "{title}"}}
                    ]}}}}}},
                    {{"musicResponsiveListItemFlexColumnRenderer": {{"text": {{"runs": [
                        {{"text": "{artist}"}}
                    ]}}}}}}
                ],
                "fixedColumns": [
                    {{"musicResponsiveListItemFixedColumnRenderer": {{"text": {{"runs": [{{"text": "{duration}"}}]}}}}}}
                ],
                "thumbnailRenderer": {{"musicThumbnailRenderer": {{"thumbnail": {{"thumbnails": [
                    {{"url": "https://img.example/{video_id}.jpg"}}
                ]}}}}}}
            }}}}"#
        )
    }

    fn detail_header(subtitle: &str, second_subtitle: &str, description: &str) -> String {
        format!(
            r#"{{"musicDetailHeaderRenderer": {{
                "title": {{"runs": [{{"text": "Lautstark"}}]}},
                "subtitle": {{"runs": [{{"text": "{subtitle}"}}]}},
                "secondSubtitle": {{"runs": [{{"text": "{second_subtitle}"}}]}},
                "description": {{"runs": [{{"text": "{description}"}}]}},
                "thumbnail": {{"croppedSquareThumbnailRenderer": {{"thumbnail": {{"thumbnails": [
                    {{"url": "https://img.example/poster.jpg"}}
                ]}}}}}}
            }}}}"#
        )
    }

    fn body(header: &str, tracks: &str) -> String {
        format!(
            r#"{{"contents": {{"singleColumnBrowseResultsRenderer": {{"tabs": [{{"tabRenderer": {{"content": {{"sectionListRenderer": {{"contents": [
                {{"musicShelfRenderer": {{"contents": [{tracks}]}}}}
            ]}}}}}}}}]}}}}, "header": {header}}}"#
        )
    }

    #[test]
    fn the_header_and_tracks_are_parsed() {
        let header = detail_header("Playlist", "2 songs", "Loud guitars.");
        let tracks = format!(
            "{},{}",
            track_row("Song One", "id1", "Artist A", "3:01"),
            track_row("Song Two", "id2", "Artist B", "2:45"),
        );

        let detail = parse_playlist_detail(&body(&header, &tracks)).unwrap();

        assert_eq!(detail.title, "Lautstark");
        assert_eq!(detail.description.as_deref(), Some("Loud guitars."));
        assert_eq!(
            detail.thumbnail_url.as_deref(),
            Some("https://img.example/poster.jpg")
        );
        assert_eq!(detail.track_count, Some(2));
        assert_eq!(detail.year, None);
        assert_eq!(detail.total_duration, None);
        assert_eq!(detail.privacy, None);
        assert_eq!(
            detail.tracks,
            [
                Track {
                    id: "id1".into(),
                    title: "Song One".into(),
                    artist: Some("Artist A".into()),
                    artist_id: None,
                    album: None,
                    duration: Some("3:01".into()),
                    thumbnail_url: Some("https://img.example/id1.jpg".into()),
                    credits: Vec::new(),
                },
                Track {
                    id: "id2".into(),
                    title: "Song Two".into(),
                    artist: Some("Artist B".into()),
                    artist_id: None,
                    album: None,
                    duration: Some("2:45".into()),
                    thumbnail_url: Some("https://img.example/id2.jpg".into()),
                    credits: Vec::new(),
                },
            ]
        );
    }

    #[test]
    fn a_third_flex_column_is_parsed_as_the_album() {
        let track = r#"{"musicResponsiveListItemRenderer": {
            "flexColumns": [
                {"musicResponsiveListItemFlexColumnRenderer": {"text": {"runs": [
                    {"navigationEndpoint": {"watchEndpoint": {"videoId": "id1"}}, "text": "Yodaka"}
                ]}}},
                {"musicResponsiveListItemFlexColumnRenderer": {"text": {"runs": [
                    {"text": "Kenshi Yonezu"}
                ]}}},
                {"musicResponsiveListItemFlexColumnRenderer": {"text": {"runs": [
                    {"text": "STRAY SHEEP"}
                ]}}}
            ]
        }}"#;

        let detail = parse_playlist_detail(&body(&detail_header("", "", ""), track)).unwrap();

        assert_eq!(detail.tracks[0].album.as_deref(), Some("STRAY SHEEP"));
    }

    #[test]
    fn a_header_whose_subtitle_has_no_recognizable_song_count_falls_back_to_the_parsed_tracks() {
        let header = detail_header("Playlist", "4+ hours", "");
        let tracks = format!(
            "{},{}",
            track_row("Song One", "id1", "Artist A", "3:01"),
            track_row("Song Two", "id2", "Artist B", "2:45"),
        );

        let detail = parse_playlist_detail(&body(&header, &tracks)).unwrap();

        assert_eq!(detail.track_count, Some(2));
    }

    #[test]
    fn the_year_and_total_duration_are_read_off_the_trailing_subtitle_segments() {
        let header = detail_header(
            "Playlist • YouTube Music • 2022",
            "87 songs • 5 hours, 20 minutes",
            "",
        );

        let detail = parse_playlist_detail(&body(&header, "")).unwrap();

        assert_eq!(detail.year.as_deref(), Some("2022"));
        assert_eq!(detail.total_duration.as_deref(), Some("5 hours, 20 minutes"));
    }

    #[test]
    fn a_trailing_segment_that_is_not_four_digits_is_not_mistaken_for_a_year() {
        let header = detail_header("Playlist • YouTube Music", "2 songs", "");

        let detail = parse_playlist_detail(&body(&header, "")).unwrap();

        assert_eq!(detail.year, None);
    }

    #[test]
    fn a_single_segment_subtitle_has_no_trailing_segment_to_pull_a_duration_from() {
        let header = detail_header("Playlist", "2 songs", "");

        let detail = parse_playlist_detail(&body(&header, "")).unwrap();

        assert_eq!(detail.total_duration, None);
    }

    #[test]
    fn the_edit_header_s_privacy_is_parsed_when_present() {
        let header = r#"{"musicPlaylistEditHeaderRenderer": {"privacy": "PRIVATE"}}"#;

        let detail = parse_playlist_detail(&body(header, "")).unwrap();

        assert_eq!(detail.privacy, Some(PlaylistPrivacy::Private));
    }

    #[test]
    fn privacy_is_absent_without_an_edit_header() {
        let detail = parse_playlist_detail(&body(&detail_header("", "", ""), "")).unwrap();

        assert_eq!(detail.privacy, None);
    }

    #[test]
    fn a_description_nested_in_a_shelf_is_found_too() {
        let header = r#"{"musicResponsiveHeaderRenderer": {
            "title": {"runs": [{"text": "Lofi Loft"}]},
            "description": {"musicDescriptionShelfRenderer": {
                "description": {"runs": [{"text": "Chill beats."}]}
            }}
        }}"#;

        let detail = parse_playlist_detail(&body(header, "")).unwrap();

        assert_eq!(detail.description.as_deref(), Some("Chill beats."));
    }

    #[test]
    fn a_track_row_without_a_video_id_is_skipped() {
        let tracks = r#"{"musicResponsiveListItemRenderer": {
            "flexColumns": [
                {"musicResponsiveListItemFlexColumnRenderer": {"text": {"runs": [{"text": "Unavailable"}]}}}
            ]
        }}"#;

        let detail = parse_playlist_detail(&body(&detail_header("", "", ""), tracks)).unwrap();

        assert_eq!(detail.tracks, []);
    }

    #[test]
    fn a_header_that_is_entirely_missing_still_parses_an_empty_title() {
        let detail = parse_playlist_detail(r#"{"contents": {}}"#).unwrap();

        assert_eq!(detail.title, "");
        assert_eq!(detail.description, None);
        assert_eq!(detail.tracks, []);
    }

    #[test]
    fn something_that_is_not_json_is_an_error() {
        assert!(parse_playlist_detail("<html>").is_err());
    }
}
