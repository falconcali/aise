use thiserror::Error;

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("llm call cancelled")]
    Cancelled,

    #[error("turn deadline exceeded")]
    TurnDeadlineExceeded,

    #[error("provider timeout")]
    ProviderTimeout,

    #[error("queue timeout")]
    QueueTimeout,

    #[error("rate limited")]
    RateLimited,

    #[error("token budget exceeded: {0}")]
    TokenBudgetExceeded(String),

    #[error("provider rejected request: {0}")]
    ProviderRejected(String),

    #[error("transport error: {0}")]
    Transport(#[from] reqwest::Error),

    #[error("protocol error: {0}")]
    Protocol(String),

    #[error("embedding not supported by provider")]
    EmbeddingUnsupported,
}

impl LlmError {
    pub fn kind(&self) -> &'static str {
        match self {
            LlmError::Cancelled => "cancelled",
            LlmError::TurnDeadlineExceeded => "turn_deadline_exceeded",
            LlmError::ProviderTimeout => "provider_timeout",
            LlmError::QueueTimeout => "queue_timeout",
            LlmError::RateLimited => "rate_limited",
            LlmError::TokenBudgetExceeded(_) => "token_budget_exceeded",
            LlmError::ProviderRejected(_) => "provider_rejected",
            LlmError::Transport(_) => "transport",
            LlmError::Protocol(_) => "protocol",
            LlmError::EmbeddingUnsupported => "embedding_unsupported",
        }
    }
}
