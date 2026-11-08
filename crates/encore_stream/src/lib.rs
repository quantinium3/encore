mod rustypipe_source;
mod yt_dlp;

use std::collections::HashMap;

use thiserror::Error;

use rustypipe_source::{RustypipeError, RustypipeSource};
use yt_dlp::{YtDlp, YtDlpError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamSource {
    Rustypipe,
    YtDlp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedStream {
    pub url: String,
    pub headers: HashMap<String, String>,
    pub source: StreamSource,
}

#[derive(Debug, Error)]
pub enum ResolveError {
    #[error(
        "no playable stream for {video_id}: rustypipe failed ({rustypipe}); yt-dlp failed ({yt_dlp})"
    )]
    NotFound {
        video_id: String,
        rustypipe: RustypipeError,
        yt_dlp: YtDlpError,
    },
    #[error("could not prepare the stream resolver: {0}")]
    Unavailable(#[source] RustypipeError),
}

pub struct StreamResolver {
    rustypipe: RustypipeSource,
}

impl StreamResolver {
    pub fn new() -> Result<Self, ResolveError> {
        Ok(Self {
            rustypipe: RustypipeSource::new().map_err(ResolveError::Unavailable)?,
        })
    }

    pub fn resolve(&self, video_id: &str) -> Result<ResolvedStream, ResolveError> {
        match self.rustypipe.resolve(video_id) {
            Ok(stream) => Ok(stream),
            Err(rustypipe_error) => match YtDlp::new().resolve(video_id) {
                Ok(stream) => Ok(stream),
                Err(yt_dlp_error) => Err(ResolveError::NotFound {
                    video_id: video_id.to_owned(),
                    rustypipe: rustypipe_error,
                    yt_dlp: yt_dlp_error,
                }),
            },
        }
    }

    pub fn resolve_via_yt_dlp(&self, video_id: &str) -> Result<ResolvedStream, YtDlpError> {
        YtDlp::new().resolve(video_id)
    }
}
