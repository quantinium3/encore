use encore_core::Track;
use serde_json::Value;

use crate::identity::{artist_id, find_all, text};
use crate::playlists::thumbnail_url;

const SEPARATOR: &str = "•";

pub(crate) const RADIO_PLAYLIST_PREFIX: &str = "RDAMVM";
pub(crate) const RADIO_PARAMS: &str = "wAEB";

pub(crate) fn parse_radio(body: &str, seed: &str) -> Result<Vec<Track>, serde_json::Error> {
    let root: Value = serde_json::from_str(body)?;
    Ok(find_all(&root, "playlistPanelVideoRenderer")
        .into_iter()
        .filter_map(parse_panel_row)
        .filter(|track| track.id != seed)
        .collect())
}

fn parse_panel_row(item: &Value) -> Option<Track> {
    let id = item.get("videoId")?.as_str()?.to_owned();
    let title = text(item.get("title"))?;
    let artist_id = artist_id(item.get("longBylineText"));
    let mut parts = segments(item.get("longBylineText")).into_iter();
    let artist = parts.next();
    let album = parts.next();

    Some(Track {
        id,
        title,
        artist,
        artist_id,
        album,
        duration: text(item.get("lengthText")),
        thumbnail_url: thumbnail_url(item),
        credits: Vec::new(),
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn row(video_id: &str, title: &str) -> String {
        format!(
            r#"{{"playlistPanelVideoRenderer": {{
                "videoId": "{video_id}",
                "title": {{"runs": [{{"text": "{title}"}}]}},
                "longBylineText": {{"runs": [
                    {{"text": "Kenshi Yonezu"}}, {{"text": " • "}}, {{"text": "Lemon"}}, {{"text": " • "}}, {{"text": "2018"}}
                ]}},
                "lengthText": {{"runs": [{{"text": "3:54"}}]}},
                "thumbnail": {{"thumbnails": [{{"url": "https://img.example/{video_id}.jpg"}}]}}
            }}}}"#
        )
    }

    #[test]
    fn radio_rows_become_tracks_without_the_seed_itself() {
        let body = format!(
            r#"{{"contents": {{"items": [{}, {}]}}}}"#,
            row("seed", "Seed"),
            row("next", "Next up")
        );

        let tracks = parse_radio(&body, "seed").unwrap();

        assert_eq!(
            tracks,
            [Track {
                id: "next".into(),
                title: "Next up".into(),
                artist: Some("Kenshi Yonezu".into()),
                artist_id: None,
                album: Some("Lemon".into()),
                duration: Some("3:54".into()),
                thumbnail_url: Some("https://img.example/next.jpg".into()),
                credits: Vec::new(),
            }]
        );
    }
}
