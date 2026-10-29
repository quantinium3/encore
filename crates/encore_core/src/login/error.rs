use thiserror::Error;

#[derive(Debug, Error)]
pub enum LoginError {
    #[error("login was cancelled")]
    Cancelled,
    #[error("login failed: {0}")]
    Backend(#[source] Box<dyn std::error::Error + Send + Sync>),
}
