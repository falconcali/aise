use super::error::ConfigError;
use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceContentPolicy {
    #[default]
    MetadataOnly,
    RedactedContent,
    FullContent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingMode {
    Enabled,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub max_concurrent: usize,
    #[serde(default)]
    pub temperature: f32,
    #[serde(default = "default_queue_timeout_ms")]
    pub queue_timeout_ms: u64,
    #[serde(default = "default_provider_timeout_ms")]
    pub provider_timeout_ms: u64,
    #[serde(default)]
    pub requests_per_minute: Option<NonZeroU32>,
    #[serde(default)]
    pub tokens_per_minute: Option<NonZeroU32>,
    #[serde(default)]
    pub trace_content: TraceContentPolicy,
    #[serde(default)]
    pub thinking: Option<ThinkingMode>,
    #[serde(default)]
    pub price_input_per_1k_tokens: Option<i64>,
    #[serde(default)]
    pub price_cached_input_per_1k_tokens: Option<i64>,
    #[serde(default)]
    pub price_output_per_1k_tokens: Option<i64>,
    #[serde(default)]
    pub protocol: LlmProtocolLimitsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmProtocolLimitsConfig {
    pub max_sse_line_bytes: usize,
    pub max_stream_buffer_bytes: usize,
    pub max_content_bytes: usize,
    pub max_reasoning_bytes: usize,
    pub max_embedding_items: usize,
    pub max_embedding_dimensions: usize,
}

impl Default for LlmProtocolLimitsConfig {
    fn default() -> Self {
        Self {
            max_sse_line_bytes: 16 * 1024,
            max_stream_buffer_bytes: 256 * 1024,
            max_content_bytes: 64 * 1024,
            max_reasoning_bytes: 32 * 1024,
            max_embedding_items: 256,
            max_embedding_dimensions: 4096,
        }
    }
}

impl LlmProtocolLimitsConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.max_sse_line_bytes == 0 {
            return Err(ConfigError::Invalid("llm.protocol.max_sse_line_bytes must be positive".into()));
        }
        if self.max_stream_buffer_bytes == 0 {
            return Err(ConfigError::Invalid(
                "llm.protocol.max_stream_buffer_bytes must be positive".into(),
            ));
        }
        if self.max_content_bytes == 0 {
            return Err(ConfigError::Invalid("llm.protocol.max_content_bytes must be positive".into()));
        }
        if self.max_reasoning_bytes == 0 {
            return Err(ConfigError::Invalid("llm.protocol.max_reasoning_bytes must be positive".into()));
        }
        if self.max_embedding_items == 0 {
            return Err(ConfigError::Invalid("llm.protocol.max_embedding_items must be positive".into()));
        }
        if self.max_embedding_dimensions == 0 {
            return Err(ConfigError::Invalid(
                "llm.protocol.max_embedding_dimensions must be positive".into(),
            ));
        }
        Ok(())
    }
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434/v1".into(),
            api_key: None,
            model: "qwen2.5".into(),
            max_concurrent: 4,
            temperature: 0.8,
            queue_timeout_ms: default_queue_timeout_ms(),
            provider_timeout_ms: default_provider_timeout_ms(),
            requests_per_minute: None,
            tokens_per_minute: None,
            trace_content: TraceContentPolicy::MetadataOnly,
            thinking: None,
            price_input_per_1k_tokens: None,
            price_cached_input_per_1k_tokens: None,
            price_output_per_1k_tokens: None,
            protocol: LlmProtocolLimitsConfig::default(),
        }
    }
}

impl LlmConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.max_concurrent == 0 {
            return Err(ConfigError::Invalid("llm.max_concurrent must be positive".into()));
        }
        if self.base_url.trim().is_empty() {
            return Err(ConfigError::Invalid("llm.base_url must not be empty".into()));
        }
        if self.model.trim().is_empty() {
            return Err(ConfigError::Invalid("llm.model must not be empty".into()));
        }
        if self.queue_timeout_ms == 0 {
            return Err(ConfigError::Invalid("llm.queue_timeout_ms must be positive".into()));
        }
        if self.provider_timeout_ms == 0 {
            return Err(ConfigError::Invalid("llm.provider_timeout_ms must be positive".into()));
        }
        self.protocol.validate()?;
        if matches!(
            self.trace_content,
            TraceContentPolicy::RedactedContent | TraceContentPolicy::FullContent
        ) && !content_recording_allowed(std::env::var("AISE_ENV").ok().as_deref())
        {
            return Err(ConfigError::Invalid(
                "llm.trace_content=redacted_content|full_content requires AISE_ENV=development".into(),
            ));
        }
        Ok(())
    }
}

fn content_recording_allowed(runtime_env: Option<&str>) -> bool {
    runtime_env == Some("development")
}

fn default_queue_timeout_ms() -> u64 {
    5_000
}

fn default_provider_timeout_ms() -> u64 {
    30_000
}

#[cfg(test)]
#[path = "tests/config_tests.rs"]
mod tests;
