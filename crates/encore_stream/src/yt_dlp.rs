use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

use serde::Deserialize;
use thiserror::Error;

use crate::{ResolvedStream, StreamSource};

const BINARY: &str = "yt-dlp";

#[derive(Debug, Error)]
pub enum YtDlpError {
    #[error("could not run yt-dlp: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("yt-dlp exited with {status}: {stderr}")]
    Failed { status: String, stderr: String },
    #[error("yt-dlp printed no stream info")]
    EmptyOutput,
    #[error("could not read yt-dlp's output: {0}")]
    Malformed(#[source] serde_json::Error),
}

#[derive(Debug, Deserialize)]
struct InfoDict {
    url: String,
    #[serde(default)]
    http_headers: HashMap<String, String>,
}

pub(crate) struct YtDlp {
    binary: PathBuf,
}

impl YtDlp {
    pub(crate) fn new() -> Self {
        Self {
            binary: PathBuf::from(BINARY),
        }
    }

    #[cfg(test)]
    pub(crate) fn at(binary: impl Into<PathBuf>) -> Self {
        Self {
            binary: binary.into(),
        }
    }

    pub(crate) fn resolve(&self, video_id: &str) -> Result<ResolvedStream, YtDlpError> {
        let output = Command::new(&self.binary)
            .args([
                "-f",
                "bestaudio",
                "--dump-json",
                "--no-warnings",
                &watch_url(video_id),
            ])
            .output()
            .map_err(YtDlpError::Spawn)?;

        if !output.status.success() {
            return Err(YtDlpError::Failed {
                status: output.status.to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let line = stdout
            .lines()
            .find(|line| !line.trim().is_empty())
            .ok_or(YtDlpError::EmptyOutput)?;
        let info: InfoDict = serde_json::from_str(line).map_err(YtDlpError::Malformed)?;
        Ok(ResolvedStream {
            url: info.url,
            headers: info.http_headers,
            source: StreamSource::YtDlp,
        })
    }
}

fn watch_url(video_id: &str) -> String {
    format!("https://music.youtube.com/watch?v={video_id}")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt as _;
    use std::sync::{Mutex, MutexGuard};

    use super::*;

    static FAKE_BINARY_LOCK: Mutex<()> = Mutex::new(());

    fn serialize_fake_binaries() -> MutexGuard<'static, ()> {
        FAKE_BINARY_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn fake_yt_dlp(dir: &Path, script: &str) -> PathBuf {
        let path = dir.join("fake-yt-dlp");
        fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn info_dict(url: &str, headers: &str) -> String {
        format!(r#"{{"url": "{url}", "http_headers": {headers}}}"#)
    }

    #[test]
    fn the_url_and_headers_are_read_from_the_dumped_json() {
        let _guard = serialize_fake_binaries();
        let dir = tempfile::tempdir().unwrap();
        let json = info_dict(
            "https://example.com/stream.webm",
            r#"{"User-Agent": "yt-dlp-agent"}"#,
        );
        let binary = fake_yt_dlp(dir.path(), &format!("echo '{json}'"));

        let resolved = YtDlp::at(binary).resolve("abc123").unwrap();

        assert_eq!(resolved.url, "https://example.com/stream.webm");
        assert_eq!(
            resolved.headers.get("User-Agent").map(String::as_str),
            Some("yt-dlp-agent")
        );
    }

    #[test]
    fn missing_http_headers_is_an_empty_map_not_an_error() {
        let _guard = serialize_fake_binaries();
        let dir = tempfile::tempdir().unwrap();
        let binary = fake_yt_dlp(
            dir.path(),
            r#"echo '{"url": "https://example.com/a"}'"#,
        );

        let resolved = YtDlp::at(binary).resolve("abc123").unwrap();

        assert!(resolved.headers.is_empty());
    }

    #[test]
    fn the_video_id_reaches_a_youtube_music_watch_url() {
        let _guard = serialize_fake_binaries();
        let dir = tempfile::tempdir().unwrap();
        let seen = dir.path().join("seen-url");
        let binary = fake_yt_dlp(
            dir.path(),
            &format!(
                r#"echo "$5" > {}; echo '{{"url": "x"}}'"#,
                seen.display()
            ),
        );

        YtDlp::at(binary).resolve("abc123").unwrap();

        assert_eq!(
            fs::read_to_string(seen).unwrap().trim(),
            "https://music.youtube.com/watch?v=abc123"
        );
    }

    #[test]
    fn a_failing_exit_reports_the_status_and_stderr() {
        let _guard = serialize_fake_binaries();
        let dir = tempfile::tempdir().unwrap();
        let binary = fake_yt_dlp(dir.path(), "echo 'video unavailable' >&2; exit 1");

        let error = YtDlp::at(binary).resolve("abc123").unwrap_err();

        assert!(matches!(error, YtDlpError::Failed { .. }));
        assert!(error.to_string().contains("video unavailable"));
    }

    #[test]
    fn empty_output_is_reported_rather_than_an_empty_url() {
        let _guard = serialize_fake_binaries();
        let dir = tempfile::tempdir().unwrap();
        let binary = fake_yt_dlp(dir.path(), "true");

        let error = YtDlp::at(binary).resolve("abc123").unwrap_err();

        assert!(matches!(error, YtDlpError::EmptyOutput));
    }

    #[test]
    fn malformed_json_is_reported_not_panicked() {
        let _guard = serialize_fake_binaries();
        let dir = tempfile::tempdir().unwrap();
        let binary = fake_yt_dlp(dir.path(), "echo 'not json'");

        let error = YtDlp::at(binary).resolve("abc123").unwrap_err();

        assert!(matches!(error, YtDlpError::Malformed(_)));
    }

    #[test]
    fn a_missing_binary_is_reported_not_panicked() {
        let error = YtDlp::at("/nonexistent/yt-dlp").resolve("abc123").unwrap_err();

        assert!(matches!(error, YtDlpError::Spawn(_)));
    }
}
