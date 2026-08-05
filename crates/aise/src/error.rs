use crate::llm::LlmError;
use crate::persistence::StoreError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AiseError {
    #[error("invalid request: {0}")]
    InvalidRequest(String),

    #[error("story {0} not found")]
    StoryNotFound(String),

    #[error("llm error: {0}")]
    Llm(#[from] LlmError),

    #[error("validation rejected: {0}")]
    ValidationRejected(String),

    #[error("validation failed after {0} repair rounds; giving up")]
    ValidationBudgetExhausted(u32),

    #[error("turn deadline exceeded")]
    TurnDeadlineExceeded,

    #[error("cancelled")]
    Cancelled,

    #[error("revision conflict")]
    RevisionConflict,

    #[error("idempotency conflict")]
    IdempotencyConflict,

    #[error("backpressure: {0}")]
    Backpressure(String),

    #[error("token budget exceeded: {0}")]
    TokenBudgetExceeded(String),

    #[error("store error: {0}")]
    Store(#[from] StoreError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("invariant violation: {0}")]
    InvariantViolation(String),

    #[error("internal: {0}")]
    Internal(String),
}
