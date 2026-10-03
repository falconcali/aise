use thiserror::Error;

#[derive(Error, Debug)]
pub enum LlmError {
    #[error("llm call cancelled")]
    Cancelled,
    #[error("llm call timed out")]
    Timeout,
    #[error("llm transport failed: {message}")]
    Transport { message: String },
    #[error("provider rejected request with status {status}: {message:?}")]
    Rejected { status: u16, message: Option<String> },
    #[error("invalid provider response: {reason}")]
    InvalidResponse { reason: &'static str },
    #[error("provider returned empty completion")]
    EmptyCompletion,
}

impl LlmError {
    pub fn kind(&self) -> &'static str {
        match self {
            LlmError::Cancelled => "cancelled",
            LlmError::Timeout => "timeout",
            LlmError::Transport { .. } => "transport",
            LlmError::Rejected { .. } => "rejected",
            LlmError::InvalidResponse { .. } => "invalid_response",
            LlmError::EmptyCompletion => "empty_completion",
        }
    }
}
