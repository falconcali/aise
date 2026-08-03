use thiserror::Error;

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("provider request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("provider returned an unexpected payload: {0}")]
    Protocol(String),

    #[error("rate limited or concurrency budget exhausted")]
    Backpressure,
}
