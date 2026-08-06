use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmTransportErrorKind {
    Connect,
    Timeout,
    Io,
    Serialization,
    Server,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmProtocolErrorKind {
    InvalidJson,
    EmptyChoices,
    InvalidSseLine,
    StreamTooLarge,
    UsageMissing,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmResponseLimit {
    Content,
    Reasoning,
    SseLine,
    StreamBuffer,
    EmbeddingDimensions,
    EmbeddingItems,
}

impl LlmResponseLimit {
    pub fn as_str(&self) -> &'static str {
        match self {
            LlmResponseLimit::Content => "content",
            LlmResponseLimit::Reasoning => "reasoning",
            LlmResponseLimit::SseLine => "sse_line",
            LlmResponseLimit::StreamBuffer => "stream_buffer",
            LlmResponseLimit::EmbeddingDimensions => "embedding_dimensions",
            LlmResponseLimit::EmbeddingItems => "embedding_items",
        }
    }
}

#[derive(Debug, Error)]
pub enum LlmProviderError {
    #[error("provider rate limited")]
    RateLimited { retry_after_ms: Option<u64> },

    #[error("provider rejected request with status {status}")]
    Rejected { status: u16, code: Option<String> },

    #[error("transport error: {kind:?}")]
    Transport { kind: LlmTransportErrorKind },

    #[error("protocol error: {kind:?}")]
    Protocol { kind: LlmProtocolErrorKind },

    #[error("response limit exceeded: {limit:?}")]
    ResponseLimitExceeded { limit: LlmResponseLimit },
}

impl LlmProviderError {
    pub fn is_rate_limited(&self) -> bool {
        matches!(self, LlmProviderError::RateLimited { .. })
    }

    pub fn retry_after_ms(&self) -> Option<u64> {
        match self {
            LlmProviderError::RateLimited { retry_after_ms } => *retry_after_ms,
            _ => None,
        }
    }
}

impl From<LlmError> for crate::core::turn_error::TurnExecutionError {
    fn from(error: LlmError) -> Self {
        match error {
            LlmError::Cancelled => Self::cancelled(None),
            LlmError::TurnDeadlineExceeded => Self::deadline_exceeded(None),
            other => Self::new(
                crate::core::turn_error::TurnFailureKind::Llm,
                "llm_error",
                None,
                other.to_string(),
            ),
        }
    }
}

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
    RateLimited { retry_after_ms: Option<u64> },

    #[error("token budget exceeded: {0}")]
    TokenBudgetExceeded(String),

    #[error("provider rejected request with status {status}")]
    ProviderRejected { status: u16 },

    #[error("transport error: {kind:?}")]
    Transport { kind: LlmTransportErrorKind },

    #[error("protocol error: {kind:?}")]
    Protocol { kind: LlmProtocolErrorKind },

    #[error("response limit exceeded: {limit:?}")]
    ResponseLimitExceeded { limit: LlmResponseLimit },

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
            LlmError::RateLimited { .. } => "rate_limited",
            LlmError::TokenBudgetExceeded(_) => "token_budget_exceeded",
            LlmError::ProviderRejected { .. } => "provider_rejected",
            LlmError::Transport { .. } => "transport",
            LlmError::Protocol { .. } => "protocol",
            LlmError::ResponseLimitExceeded { .. } => "response_limit_exceeded",
            LlmError::EmbeddingUnsupported => "embedding_unsupported",
        }
    }
}

impl From<LlmProviderError> for LlmError {
    fn from(error: LlmProviderError) -> Self {
        match error {
            LlmProviderError::RateLimited { retry_after_ms } => LlmError::RateLimited { retry_after_ms },
            LlmProviderError::Rejected { status, .. } => LlmError::ProviderRejected { status },
            LlmProviderError::Transport { kind } => LlmError::Transport { kind },
            LlmProviderError::Protocol { kind } => LlmError::Protocol { kind },
            LlmProviderError::ResponseLimitExceeded { limit } => LlmError::ResponseLimitExceeded { limit },
        }
    }
}
