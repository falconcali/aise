use super::error::ConfigError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructuredOutputMode {
    NativeJsonSchema,
    ForcedStrictTool,
    JsonObject,
    PromptFallback,
}

impl StructuredOutputMode {
    pub const PREFERENCE_ORDER: [StructuredOutputMode; 4] = [
        StructuredOutputMode::NativeJsonSchema,
        StructuredOutputMode::ForcedStrictTool,
        StructuredOutputMode::JsonObject,
        StructuredOutputMode::PromptFallback,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::NativeJsonSchema => "native_json_schema",
            Self::ForcedStrictTool => "forced_strict_tool",
            Self::JsonObject => "json_object",
            Self::PromptFallback => "prompt_fallback",
        }
    }

    pub fn injects_prompt_contract(self) -> bool {
        matches!(self, Self::JsonObject | Self::PromptFallback)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelStructuredOutputCapabilities {
    pub provider: String,
    pub model: String,
    pub supported_modes: Vec<StructuredOutputMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructuredOutputConfig {
    #[serde(default = "default_structured_output_modes")]
    pub default_modes: Vec<StructuredOutputMode>,
    #[serde(default)]
    pub model_capabilities: Vec<ModelStructuredOutputCapabilities>,
}

fn default_structured_output_modes() -> Vec<StructuredOutputMode> {
    vec![StructuredOutputMode::PromptFallback]
}

impl Default for StructuredOutputConfig {
    fn default() -> Self {
        Self {
            default_modes: default_structured_output_modes(),
            model_capabilities: Vec::new(),
        }
    }
}

impl StructuredOutputConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.default_modes.is_empty() {
            return Err(ConfigError::Invalid(
                "llm.structured_output.default_modes must not be empty".into(),
            ));
        }
        if has_duplicate_modes(&self.default_modes) {
            return Err(ConfigError::Invalid(
                "llm.structured_output.default_modes contains a duplicate mode".into(),
            ));
        }
        let mut seen_overrides = BTreeSet::new();
        for entry in &self.model_capabilities {
            if entry.provider.trim().is_empty() || entry.model.trim().is_empty() {
                return Err(ConfigError::Invalid(
                    "llm.structured_output.model_capabilities entry has an empty provider or model".into(),
                ));
            }
            if entry.supported_modes.is_empty() {
                return Err(ConfigError::Invalid(
                    "llm.structured_output.model_capabilities entry has an empty supported_modes list".into(),
                ));
            }
            if has_duplicate_modes(&entry.supported_modes) {
                return Err(ConfigError::Invalid(
                    "llm.structured_output.model_capabilities entry has a duplicate supported mode".into(),
                ));
            }
            if !seen_overrides.insert((entry.provider.clone(), entry.model.clone())) {
                return Err(ConfigError::Invalid(
                    "llm.structured_output.model_capabilities has a duplicate (provider, model) entry".into(),
                ));
            }
        }
        Ok(())
    }

    pub fn configured_modes(&self, provider: &str, model: &str) -> &[StructuredOutputMode] {
        self.model_capabilities
            .iter()
            .find(|entry| entry.provider == provider && entry.model == model)
            .map(|entry| entry.supported_modes.as_slice())
            .unwrap_or(self.default_modes.as_slice())
    }
}

fn has_duplicate_modes(modes: &[StructuredOutputMode]) -> bool {
    let mut seen = BTreeSet::new();
    !modes.iter().all(|mode| seen.insert(*mode))
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
    #[serde(default)]
    pub structured_output: StructuredOutputConfig,
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
            structured_output: StructuredOutputConfig::default(),
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
        self.structured_output.validate()?;
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
