use std::collections::HashMap;

use rustypipe::model::AudioStream;
use rustypipe::model::traits::QualityOrd as _;
use rustypipe::client::RustyPipe;
use thiserror::Error;
use tokio::runtime::Runtime;

use crate::{ResolvedStream, StreamSource};

const ORIGIN: &str = "https://www.youtube.com";
const REFERER: &str = "https://www.youtube.com/";

#[derive(Debug, Error)]
pub enum RustypipeError {
    #[error("could not start the async runtime: {0}")]
    Runtime(#[source] std::io::Error),
    #[error(transparent)]
    Client(#[from] rustypipe::error::Error),
    #[error("no audio-only stream was offered for this track")]
    NoAudioStream,
}

pub(crate) struct RustypipeSource {
    client: RustyPipe,
    runtime: Runtime,
}

impl RustypipeSource {
    pub(crate) fn new() -> Result<Self, RustypipeError> {
        let runtime = Runtime::new().map_err(RustypipeError::Runtime)?;
        Ok(Self {
            client: RustyPipe::new(),
            runtime,
        })
    }

    pub(crate) fn resolve(&self, video_id: &str) -> Result<ResolvedStream, RustypipeError> {
        let query = self.client.query();
        let player = self.runtime.block_on(query.player(video_id))?;
        let url =
            best_audio_url(player.audio_streams).ok_or(RustypipeError::NoAudioStream)?;
        let headers = HashMap::from([
            (
                "User-Agent".to_owned(),
                query.user_agent(player.client_type).into_owned(),
            ),
            ("Origin".to_owned(), ORIGIN.to_owned()),
            ("Referer".to_owned(), REFERER.to_owned()),
        ]);
        Ok(ResolvedStream {
            url,
            headers,
            source: StreamSource::Rustypipe,
        })
    }
}

fn best_audio_url(streams: Vec<AudioStream>) -> Option<String> {
    streams
        .into_iter()
        .max_by(|a, b| a.quality_cmp(b))
        .map(|stream| stream.url)
}
