use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum Error {
    #[error("the request to YouTube failed")]
    Http(#[source] ureq::Error),
    #[error("YouTube answered with HTTP {0}")]
    Status(u16),
    #[error("YouTube sent a response that is not valid JSON")]
    Response(#[source] serde_json::Error),
    #[error("YouTube's response does not say which account the session belongs to ({0})")]
    NoIdentity(String),
}
