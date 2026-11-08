mod options;
mod volume;

use std::collections::HashMap;
use std::sync::Arc;
use std::thread;

use libmpv2::events::{Event, EventContext, PropertyData};
use libmpv2::mpv_end_file_reason;
use libmpv2::{Format, Mpv};
use thiserror::Error;

use options::configure;
use volume::perceptual_to_mpv;

#[derive(Debug, Error)]
pub enum PlayerError {
    #[error("mpv: {0}")]
    Mpv(#[from] libmpv2::Error),
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlayerEvent {
    Position(f64),
    Duration(f64),
    Playing(bool),
    TrackEnded,
    TrackFailed(String),
}

pub struct Player {
    mpv: Arc<Mpv>,
}

impl Player {
    pub fn new(on_event: impl Fn(PlayerEvent) + Send + Sync + 'static) -> Result<Self, PlayerError> {
        unsafe {
            libc::setlocale(libc::LC_NUMERIC, c"C".as_ptr());
        }

        let mpv = Arc::new(Mpv::new()?);
        configure(&mpv)?;
        spawn_event_loop(mpv.clone(), Box::new(on_event));
        Ok(Self { mpv })
    }

    pub fn load(&self, url: &str, headers: &HashMap<String, String>) -> Result<(), PlayerError> {
        self.mpv
            .set_property("user-agent", resolve_user_agent(headers))?;
        self.mpv
            .set_property("http-header-fields", header_fields(headers).as_str())?;
        self.mpv.command("loadfile", &[&quoted(url), "replace"])?;
        Ok(())
    }

    pub fn enqueue(&self, url: &str, headers: &HashMap<String, String>) -> Result<(), PlayerError> {
        self.mpv
            .set_property("user-agent", resolve_user_agent(headers))?;
        self.mpv
            .set_property("http-header-fields", header_fields(headers).as_str())?;
        self.mpv.command("loadfile", &[&quoted(url), "append"])?;
        Ok(())
    }

    pub fn clear_lookahead(&self) -> Result<(), PlayerError> {
        self.mpv.command("playlist-clear", &[])?;
        Ok(())
    }

    pub fn is_idle(&self) -> bool {
        self.mpv.get_property::<bool>("idle-active").unwrap_or(true)
    }

    pub fn play(&self) -> Result<(), PlayerError> {
        self.mpv.set_property("pause", false)?;
        Ok(())
    }

    pub fn pause(&self) -> Result<(), PlayerError> {
        self.mpv.set_property("pause", true)?;
        Ok(())
    }

    pub fn toggle_play_pause(&self) -> Result<(), PlayerError> {
        self.mpv.command("cycle", &["pause"])?;
        Ok(())
    }

    pub fn stop(&self) -> Result<(), PlayerError> {
        self.mpv.command("stop", &[])?;
        Ok(())
    }

    pub fn seek(&self, position_secs: f64) -> Result<(), PlayerError> {
        self.mpv
            .command("seek", &[&position_secs.to_string(), "absolute"])?;
        Ok(())
    }

    pub fn set_volume(&self, percent: i64) -> Result<(), PlayerError> {
        self.mpv.set_property("volume", perceptual_to_mpv(percent))?;
        Ok(())
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.mpv.command("quit", &[]);
    }
}

fn spawn_event_loop(mpv: Arc<Mpv>, on_event: Box<dyn Fn(PlayerEvent) + Send + Sync>) {
    thread::spawn(move || {
        let mut events = EventContext::new(mpv.ctx);
        let _ = events.disable_deprecated_events();
        let _ = events.observe_property("time-pos", Format::Double, 0);
        let _ = events.observe_property("duration", Format::Double, 1);
        let _ = events.observe_property("pause", Format::Flag, 2);

        let mut paused = false;
        let mut started = false;
        let mut playing = false;

        loop {
            match events.wait_event(1.0) {
                Some(Ok(Event::Shutdown)) => break,
                Some(Ok(event)) => {
                    let emitted = match event {
                        Event::PropertyChange {
                            name: "time-pos",
                            change: PropertyData::Double(position),
                            ..
                        } => Some(PlayerEvent::Position(position)),
                        Event::PropertyChange {
                            name: "duration",
                            change: PropertyData::Double(duration),
                            ..
                        } => Some(PlayerEvent::Duration(duration)),
                        Event::PropertyChange {
                            name: "pause",
                            change: PropertyData::Flag(flag),
                            ..
                        } => {
                            paused = flag;
                            None
                        }
                        Event::StartFile => {
                            started = false;
                            None
                        }
                        Event::PlaybackRestart => {
                            started = true;
                            None
                        }
                        Event::EndFile(reason) if reason == mpv_end_file_reason::Eof => {
                            Some(PlayerEvent::TrackEnded)
                        }
                        _ => None,
                    };
                    if let Some(event) = emitted {
                        on_event(event);
                    }

                    let now_playing = started && !paused;
                    if now_playing != playing {
                        playing = now_playing;
                        on_event(PlayerEvent::Playing(playing));
                    }
                }
                Some(Err(error)) => on_event(PlayerEvent::TrackFailed(error.to_string())),
                None => {}
            }
        }
    });
}

const DEFAULT_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) \
     Chrome/120.0.0.0 Safari/537.36";

fn resolve_user_agent(headers: &HashMap<String, String>) -> &str {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("user-agent"))
        .map_or(DEFAULT_USER_AGENT, |(_, value)| value.as_str())
}

const FORWARDED_HEADERS: [&str; 2] = ["Origin", "Referer"];

fn header_fields(headers: &HashMap<String, String>) -> String {
    headers
        .iter()
        .filter(|(key, _)| {
            FORWARDED_HEADERS
                .iter()
                .any(|allowed| key.eq_ignore_ascii_case(allowed))
        })
        .map(|(key, value)| {
            let field = format!("{key}: {value}");
            format!("%{}%{field}", field.len())
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn quoted(arg: &str) -> String {
    format!("\"{}\"", arg.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_url_is_wrapped_in_quotes() {
        assert_eq!(quoted("https://example.com/a"), "\"https://example.com/a\"");
    }

    #[test]
    fn backslashes_and_quotes_are_escaped() {
        assert_eq!(quoted(r#"a\b"c"#), r#""a\\b\"c""#);
    }

    #[test]
    fn a_player_can_be_built_and_shut_down() {
        let player = Player::new(|_| {}).expect("mpv is installed in this environment");
        drop(player);
    }

    #[test]
    fn a_missing_user_agent_falls_back_to_the_default() {
        assert_eq!(resolve_user_agent(&HashMap::new()), DEFAULT_USER_AGENT);
    }

    #[test]
    fn a_given_user_agent_is_used_as_is() {
        let headers = HashMap::from([("User-Agent".to_owned(), "custom-agent".to_owned())]);

        assert_eq!(resolve_user_agent(&headers), "custom-agent");
    }

    #[test]
    fn the_user_agent_lookup_is_case_insensitive() {
        let headers = HashMap::from([("user-agent".to_owned(), "custom-agent".to_owned())]);

        assert_eq!(resolve_user_agent(&headers), "custom-agent");
    }

    #[test]
    fn header_fields_excludes_the_user_agent() {
        let headers = HashMap::from([
            ("User-Agent".to_owned(), "custom-agent".to_owned()),
            ("Origin".to_owned(), "https://example.com".to_owned()),
        ]);

        assert_eq!(header_fields(&headers), "%27%Origin: https://example.com");
    }

    #[test]
    fn header_fields_drops_browser_navigation_headers() {
        let headers = HashMap::from([
            ("Accept".to_owned(), "text/html,*/*;q=0.8".to_owned()),
            ("Sec-Fetch-Mode".to_owned(), "navigate".to_owned()),
            ("Referer".to_owned(), "https://example.com/".to_owned()),
        ]);

        assert_eq!(header_fields(&headers), "%29%Referer: https://example.com/");
    }

    #[test]
    fn header_fields_with_nothing_but_a_user_agent_is_empty() {
        let headers = HashMap::from([("User-Agent".to_owned(), "custom-agent".to_owned())]);

        assert_eq!(header_fields(&headers), "");
    }
}
