use encore_core::Playlist;
use serde_json::Value;

use crate::identity::{find_all, find_first, text};

pub(crate) const LIBRARY_PLAYLISTS_BROWSE_ID: &str = "FEmusic_liked_playlists";

pub(crate) fn parse_library_playlists(body: &str) -> Result<Vec<Playlist>, serde_json::Error> {
    let root: Value = serde_json::from_str(body)?;
    let playlists = find_all(&root, "musicTwoRowItemRenderer")
        .into_iter()
        .filter_map(parse_playlist_card)
        .collect();
    Ok(playlists)
}

fn parse_playlist_card(item: &Value) -> Option<Playlist> {
    let browse_id = match item
        .get("navigationEndpoint")
        .and_then(|endpoint| endpoint.get("browseEndpoint"))
        .and_then(|browse| browse.get("browseId"))
        .and_then(Value::as_str)
    {
        Some(id) if id.starts_with("VL") => id,
        _ => return None,
    };
    let title = text(item.get("title"))?;
    let subtitle = text(item.get("subtitle"));

    Some(Playlist {
        id: browse_id.to_owned(),
        title,
        track_count: subtitle.as_deref().and_then(track_count),
        thumbnail_url: thumbnail_url(item),
        owner: subtitle.as_deref().and_then(owner),
    })
}

fn owner(subtitle: &str) -> Option<String> {
    let owner = subtitle
        .split(" • ")
        .map(str::trim)
        .filter(|segment| {
            !segment.is_empty()
                && !segment.eq_ignore_ascii_case("playlist")
                && track_count(segment).is_none()
        })
        .collect::<Vec<_>>()
        .join(" • ");
    (!owner.is_empty()).then_some(owner)
}

pub(crate) fn track_count(subtitle: &str) -> Option<u32> {
    let words: Vec<&str> = subtitle.split_whitespace().collect();
    words.iter().enumerate().find_map(|(index, word)| {
        match (word.parse::<u32>(), words.get(index + 1)) {
            (Ok(count), Some(next)) if starts_with_song(next) => Some(count),
            _ => None,
        }
    })
}

fn starts_with_song(word: &str) -> bool {
    word.get(..4).is_some_and(|prefix| prefix.eq_ignore_ascii_case("song"))
}

pub(crate) fn thumbnail_url(item: &Value) -> Option<String> {
    let url = find_first(item, "thumbnails")?
        .as_array()?
        .last()?
        .get("url")?
        .as_str()?
        .trim();
    match url {
        "" => None,
        url if url.starts_with("//") => Some(format!("https:{url}")),
        url => Some(url.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(browse_id: &str, title: &str, subtitle: &str, thumbnail: &str) -> String {
        format!(
            r#"{{"musicTwoRowItemRenderer": {{
                "title": {{"runs": [{{"text": "{title}"}}]}},
                "subtitle": {{"runs": [{{"text": "{subtitle}"}}]}},
                "navigationEndpoint": {{"browseEndpoint": {{"browseId": "{browse_id}"}}}},
                "thumbnailRenderer": {{"musicThumbnailRenderer": {{"thumbnail": {{"thumbnails": [
                    {{"url": "{thumbnail}"}}
                ]}}}}}}
            }}}}"#
        )
    }

    fn grid(cards: &str) -> String {
        format!(
            r#"{{"contents": {{"singleColumnBrowseResultsRenderer": {{"tabs": [{{"tabRenderer": {{"content": {{"sectionListRenderer": {{"contents": [
                {{"gridRenderer": {{"items": [{cards}]}}}}
            ]}}}}}}}}]}}}}}}"#
        )
    }

    #[test]
    fn a_playlist_card_is_parsed_into_a_playlist() {
        let body = grid(&card(
            "VLabc",
            "Lautstark",
            "YouTube Music • 62 songs",
            "https://img.example/a.jpg",
        ));

        let playlists = parse_library_playlists(&body).unwrap();

        assert_eq!(
            playlists,
            [Playlist {
                id: "VLabc".into(),
                title: "Lautstark".into(),
                track_count: Some(62),
                thumbnail_url: Some("https://img.example/a.jpg".into()),
                owner: Some("YouTube Music".into()),
            }]
        );
    }

    #[test]
    fn a_title_cased_song_count_is_still_recognized() {
        let body = grid(&card("VLabc", "Owned playlist", "5 Songs", ""));

        let playlists = parse_library_playlists(&body).unwrap();

        assert_eq!(playlists[0].track_count, Some(5));
    }

    #[test]
    fn a_card_without_a_browse_id_is_skipped() {
        let body = r#"{"contents": [{"musicTwoRowItemRenderer": {
            "title": {"runs": [{"text": "New playlist"}]},
            "navigationEndpoint": {"createPlaylistEndpoint": {}}
        }}]}"#;

        assert_eq!(parse_library_playlists(body).unwrap(), []);
    }

    #[test]
    fn a_browse_id_that_is_not_a_playlist_is_skipped() {
        let body = grid(&card("UCsomeone", "An artist", "Artist", ""));

        assert_eq!(parse_library_playlists(&body).unwrap(), []);
    }

    #[test]
    fn a_subtitle_without_a_song_count_has_no_track_count() {
        let body = grid(&card("VLLM", "Liked Music", "Auto playlist", ""));

        let playlists = parse_library_playlists(&body).unwrap();

        assert_eq!(playlists[0].track_count, None);
    }

    #[test]
    fn a_blank_thumbnail_url_is_missing_not_blank() {
        let body = grid(&card("VLabc", "Title", "1 song", ""));

        let playlists = parse_library_playlists(&body).unwrap();

        assert_eq!(playlists[0].thumbnail_url, None);
    }

    #[test]
    fn a_protocol_relative_thumbnail_url_becomes_https() {
        let body = grid(&card("VLabc", "Title", "1 song", "//img.example/a.jpg"));

        let playlists = parse_library_playlists(&body).unwrap();

        assert_eq!(
            playlists[0].thumbnail_url.as_deref(),
            Some("https://img.example/a.jpg")
        );
    }

    #[test]
    fn something_that_is_not_json_is_an_error() {
        assert!(parse_library_playlists("<html>").is_err());
    }
}
