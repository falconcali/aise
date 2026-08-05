use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AiseConfig {
    #[serde(default)]
    pub llm: LlmConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub turn: TurnConfig,
    #[serde(default)]
    pub coordinator: CoordinatorConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceContent {
    #[default]
    MetadataOnly,
    Content,
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
    pub trace_content: TraceContent,
    #[serde(default)]
    pub thinking: Option<ThinkingMode>,
    #[serde(default)]
    pub price_input_per_1k_tokens: Option<i64>,
    #[serde(default)]
    pub price_cached_input_per_1k_tokens: Option<i64>,
    #[serde(default)]
    pub price_output_per_1k_tokens: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    #[serde(default)]
    pub database_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnConfig {
    #[serde(default)]
    pub max_repair_rounds: u32,
    #[serde(default = "default_max_llm_calls")]
    pub max_llm_calls: u32,
    #[serde(default = "default_max_input_tokens")]
    pub max_input_tokens: u64,
    #[serde(default = "default_max_output_tokens", alias = "max_tokens")]
    pub max_output_tokens: u64,
    #[serde(default = "default_max_total_tokens")]
    pub max_total_tokens: u64,
    #[serde(default)]
    pub max_retrieved_items: usize,
    #[serde(default = "default_max_context_tokens")]
    pub max_context_tokens: u64,
    #[serde(default = "default_max_character_thoughts")]
    pub max_character_thoughts: usize,
    #[serde(default = "default_max_validation_issues")]
    pub max_validation_issues: usize,
    #[serde(default = "default_max_trace_spans")]
    pub max_trace_spans: usize,
    #[serde(default = "default_turn_timeout_ms")]
    pub turn_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinatorConfig {
    #[serde(default = "default_max_waiters_per_story")]
    pub max_waiters_per_story: usize,
    #[serde(default = "default_max_total_waiters")]
    pub max_total_waiters: usize,
    #[serde(default = "default_idle_timeout_secs")]
    pub idle_timeout_secs: u64,
}

fn default_queue_timeout_ms() -> u64 {
    5_000
}

fn default_provider_timeout_ms() -> u64 {
    30_000
}

fn default_max_llm_calls() -> u32 {
    8
}

fn default_max_input_tokens() -> u64 {
    8_192
}

fn default_max_output_tokens() -> u64 {
    4_096
}

fn default_max_total_tokens() -> u64 {
    12_288
}

fn default_max_context_tokens() -> u64 {
    8_192
}

fn default_max_character_thoughts() -> usize {
    8
}

fn default_max_validation_issues() -> usize {
    32
}

fn default_max_trace_spans() -> usize {
    64
}

fn default_turn_timeout_ms() -> u64 {
    60_000
}

fn default_max_waiters_per_story() -> usize {
    16
}

fn default_max_total_waiters() -> usize {
    256
}

fn default_idle_timeout_secs() -> u64 {
    300
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
            trace_content: TraceContent::MetadataOnly,
            thinking: None,
            price_input_per_1k_tokens: None,
            price_cached_input_per_1k_tokens: None,
            price_output_per_1k_tokens: None,
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            database_url: "data/aise.db".into(),
        }
    }
}

impl Default for TurnConfig {
    fn default() -> Self {
        Self {
            max_repair_rounds: 3,
            max_llm_calls: default_max_llm_calls(),
            max_input_tokens: default_max_input_tokens(),
            max_output_tokens: default_max_output_tokens(),
            max_total_tokens: default_max_total_tokens(),
            max_retrieved_items: 20,
            max_context_tokens: default_max_context_tokens(),
            max_character_thoughts: default_max_character_thoughts(),
            max_validation_issues: default_max_validation_issues(),
            max_trace_spans: default_max_trace_spans(),
            turn_timeout_ms: default_turn_timeout_ms(),
        }
    }
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            max_waiters_per_story: default_max_waiters_per_story(),
            max_total_waiters: default_max_total_waiters(),
            idle_timeout_secs: default_idle_timeout_secs(),
        }
    }
}

impl LlmConfig {
    pub fn validate(&self) -> Result<(), crate::error::AiseError> {
        if self.max_concurrent == 0 {
            return Err(crate::error::AiseError::InvalidRequest(
                "llm.max_concurrent must be positive".into(),
            ));
        }
        if self.base_url.trim().is_empty() {
            return Err(crate::error::AiseError::InvalidRequest("llm.base_url must not be empty".into()));
        }
        if self.model.trim().is_empty() {
            return Err(crate::error::AiseError::InvalidRequest("llm.model must not be empty".into()));
        }
        if self.queue_timeout_ms == 0 {
            return Err(crate::error::AiseError::InvalidRequest(
                "llm.queue_timeout_ms must be positive".into(),
            ));
        }
        if self.provider_timeout_ms == 0 {
            return Err(crate::error::AiseError::InvalidRequest(
                "llm.provider_timeout_ms must be positive".into(),
            ));
        }
        Ok(())
    }
}

impl TurnConfig {
    pub fn validate(&self) -> Result<(), crate::error::AiseError> {
        if self.max_output_tokens == 0 {
            return Err(crate::error::AiseError::InvalidRequest(
                "turn.max_output_tokens must be positive".into(),
            ));
        }
        if self.max_total_tokens < self.max_output_tokens {
            return Err(crate::error::AiseError::InvalidRequest(
                "turn.max_total_tokens must be >= turn.max_output_tokens".into(),
            ));
        }
        if self.max_context_tokens == 0 {
            return Err(crate::error::AiseError::InvalidRequest(
                "turn.max_context_tokens must be positive".into(),
            ));
        }
        Ok(())
    }
}
