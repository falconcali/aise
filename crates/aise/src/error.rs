use crate::llm::LlmError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AiseError {
    #[error("invalid request: {0}")]
    InvalidRequest(String),

    #[error("story {0} not found")]
    StoryNotFound(String),

    #[error("llm error: {0}")]
    Llm(#[from] LlmError),

    #[error("store error: {0}")]
    Store(#[from] sqlx::Error),

    #[error("store migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("validation failed after {0} repair rounds; giving up")]
    ValidationBudgetExhausted(u32),

    #[error("invariant violation: {0}")]
    InvariantViolation(String),

    #[error("internal: {0}")]
    Internal(String),
}
