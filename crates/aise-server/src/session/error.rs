use thiserror::Error;

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("session quota exceeded ({0})")]
    QuotaExceeded(usize),
    #[error("invalid story id: {0}")]
    StoryIdInvalid(String),
    #[error("session id must not be empty")]
    InvalidId,
}
