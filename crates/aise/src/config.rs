use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ConfigError {
    #[error("invalid configuration: {0}")]
    Invalid(String),
    #[error("failed to read configuration: {0}")]
    Io(String),
    #[error("failed to parse configuration: {0}")]
    Parse(String),
}

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
    #[serde(default)]
    pub content: TurnContentLimitsConfig,
    #[serde(default)]
    pub assets: AssetLimitsConfig,
    #[serde(default)]
    pub prompt: PromptModuleConfig,
}

impl AiseConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.llm.validate()?;
        self.storage.validate()?;
        self.turn.validate()?;
        self.coordinator.validate()?;
        self.content.validate()?;
        self.assets.validate()?;
        self.prompt.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceContentPolicy {
    #[default]
    MetadataOnly,
    RedactedContent,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    #[serde(default)]
    pub database_url: String,
}

impl StorageConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.database_url.trim().is_empty() {
            return Err(ConfigError::Invalid("storage.database_url must not be empty".into()));
        }
        Ok(())
    }
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
    #[serde(default = "default_max_retrieval_candidates")]
    pub max_retrieval_candidates: usize,
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

impl TurnConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.max_output_tokens == 0 {
            return Err(ConfigError::Invalid("turn.max_output_tokens must be positive".into()));
        }
        if self.max_input_tokens == 0 {
            return Err(ConfigError::Invalid("turn.max_input_tokens must be positive".into()));
        }
        if self.max_total_tokens < self.max_output_tokens {
            return Err(ConfigError::Invalid(
                "turn.max_total_tokens must be >= turn.max_output_tokens".into(),
            ));
        }
        if self.max_total_tokens < self.max_input_tokens {
            return Err(ConfigError::Invalid(
                "turn.max_total_tokens must be >= turn.max_input_tokens".into(),
            ));
        }
        if self.max_context_tokens == 0 {
            return Err(ConfigError::Invalid("turn.max_context_tokens must be positive".into()));
        }
        if self.max_llm_calls == 0 {
            return Err(ConfigError::Invalid("turn.max_llm_calls must be positive".into()));
        }
        if self.max_retrieved_items == 0 {
            return Err(ConfigError::Invalid("turn.max_retrieved_items must be positive".into()));
        }
        if self.max_retrieval_candidates == 0 {
            return Err(ConfigError::Invalid("turn.max_retrieval_candidates must be positive".into()));
        }
        if self.max_retrieval_candidates < self.max_retrieved_items {
            return Err(ConfigError::Invalid(
                "turn.max_retrieval_candidates must be >= turn.max_retrieved_items".into(),
            ));
        }
        if self.max_character_thoughts == 0 {
            return Err(ConfigError::Invalid("turn.max_character_thoughts must be positive".into()));
        }
        if self.max_validation_issues == 0 {
            return Err(ConfigError::Invalid("turn.max_validation_issues must be positive".into()));
        }
        if self.max_trace_spans == 0 {
            return Err(ConfigError::Invalid("turn.max_trace_spans must be positive".into()));
        }
        if self.turn_timeout_ms == 0 {
            return Err(ConfigError::Invalid("turn.turn_timeout_ms must be positive".into()));
        }
        Ok(())
    }
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

impl CoordinatorConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.max_waiters_per_story == 0 {
            return Err(ConfigError::Invalid(
                "coordinator.max_waiters_per_story must be positive".into(),
            ));
        }
        if self.max_total_waiters == 0 {
            return Err(ConfigError::Invalid("coordinator.max_total_waiters must be positive".into()));
        }
        if self.idle_timeout_secs == 0 {
            return Err(ConfigError::Invalid("coordinator.idle_timeout_secs must be positive".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnContentLimitsConfig {
    pub max_story_instructions_bytes: usize,
    pub max_story_config_bytes: usize,
    pub max_scene_bytes: usize,
    pub max_summary_bytes: usize,
    pub max_constraints: usize,
    pub max_constraint_bytes: usize,
    pub max_characters: usize,
    pub max_character_bytes: usize,
    pub max_world_facts: usize,
    pub max_world_fact_bytes: usize,
    pub max_recent_turns: usize,
    pub max_recent_turn_bytes: usize,
    pub max_memories: usize,
    pub max_memory_bytes: usize,
    pub max_plan_bytes: usize,
    pub max_retrieval_candidates: usize,
    pub max_retrieved_items: usize,
    pub max_retrieved_item_bytes: usize,
    pub max_retrieved_tokens: u64,
    pub max_character_thoughts: usize,
    pub max_character_thought_bytes: usize,
    pub max_proposal_bytes: usize,
    pub max_validation_issues: usize,
    pub max_validation_issue_bytes: usize,
    pub max_trace_spans: usize,
    pub max_trace_field_bytes: usize,
}

impl Default for TurnContentLimitsConfig {
    fn default() -> Self {
        Self {
            max_story_instructions_bytes: 4096,
            max_story_config_bytes: 1024,
            max_scene_bytes: 8192,
            max_summary_bytes: 4096,
            max_constraints: 16,
            max_constraint_bytes: 512,
            max_characters: 16,
            max_character_bytes: 2048,
            max_world_facts: 128,
            max_world_fact_bytes: 512,
            max_recent_turns: 20,
            max_recent_turn_bytes: 8192,
            max_memories: 32,
            max_memory_bytes: 512,
            max_plan_bytes: 4096,
            max_retrieval_candidates: 64,
            max_retrieved_items: 10,
            max_retrieved_item_bytes: 1024,
            max_retrieved_tokens: 4096,
            max_character_thoughts: 8,
            max_character_thought_bytes: 1024,
            max_proposal_bytes: 32 * 1024,
            max_validation_issues: 32,
            max_validation_issue_bytes: 500,
            max_trace_spans: 64,
            max_trace_field_bytes: 2048,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetLimitsConfig {
    pub max_key_bytes: usize,
    pub max_text_bytes: usize,
    pub max_tags_per_item: usize,
    pub max_roles: usize,
    pub max_character_assets: usize,
    pub max_world_facts: usize,
    pub max_world_rumors: usize,
    pub max_seed_memories_per_role: usize,
    pub max_relationships_per_role: usize,
    pub max_graph_nodes: usize,
    pub max_graph_edges: usize,
    pub max_condition_depth: usize,
    pub max_conditions_per_node: usize,
    pub max_effects_per_node: usize,
    pub max_manifest_bytes: usize,
    pub max_compressed_pack_bytes: u64,
    pub max_uncompressed_pack_bytes: u64,
    pub max_compression_ratio: u32,
    pub max_asset_files: usize,
    pub max_single_asset_bytes: u64,
    pub max_validation_issues: usize,
}

impl Default for AssetLimitsConfig {
    fn default() -> Self {
        Self {
            max_key_bytes: 128,
            max_text_bytes: 32 * 1024,
            max_tags_per_item: 32,
            max_roles: 32,
            max_character_assets: 64,
            max_world_facts: 512,
            max_world_rumors: 256,
            max_seed_memories_per_role: 32,
            max_relationships_per_role: 32,
            max_graph_nodes: 256,
            max_graph_edges: 512,
            max_condition_depth: 8,
            max_conditions_per_node: 16,
            max_effects_per_node: 16,
            max_manifest_bytes: 512 * 1024,
            max_compressed_pack_bytes: 32 * 1024 * 1024,
            max_uncompressed_pack_bytes: 128 * 1024 * 1024,
            max_compression_ratio: 64,
            max_asset_files: 1024,
            max_single_asset_bytes: 16 * 1024 * 1024,
            max_validation_issues: 64,
        }
    }
}

impl AssetLimitsConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.max_key_bytes == 0 {
            return Err(ConfigError::Invalid("assets.max_key_bytes must be positive".into()));
        }
        if self.max_text_bytes == 0 {
            return Err(ConfigError::Invalid("assets.max_text_bytes must be positive".into()));
        }
        if self.max_tags_per_item == 0 {
            return Err(ConfigError::Invalid("assets.max_tags_per_item must be positive".into()));
        }
        if self.max_roles == 0 {
            return Err(ConfigError::Invalid("assets.max_roles must be positive".into()));
        }
        if self.max_character_assets == 0 {
            return Err(ConfigError::Invalid("assets.max_character_assets must be positive".into()));
        }
        if self.max_world_facts == 0 {
            return Err(ConfigError::Invalid("assets.max_world_facts must be positive".into()));
        }
        if self.max_world_rumors == 0 {
            return Err(ConfigError::Invalid("assets.max_world_rumors must be positive".into()));
        }
        if self.max_seed_memories_per_role == 0 {
            return Err(ConfigError::Invalid(
                "assets.max_seed_memories_per_role must be positive".into(),
            ));
        }
        if self.max_relationships_per_role == 0 {
            return Err(ConfigError::Invalid(
                "assets.max_relationships_per_role must be positive".into(),
            ));
        }
        if self.max_graph_nodes == 0 {
            return Err(ConfigError::Invalid("assets.max_graph_nodes must be positive".into()));
        }
        if self.max_graph_edges == 0 {
            return Err(ConfigError::Invalid("assets.max_graph_edges must be positive".into()));
        }
        if self.max_condition_depth == 0 {
            return Err(ConfigError::Invalid("assets.max_condition_depth must be positive".into()));
        }
        if self.max_conditions_per_node == 0 {
            return Err(ConfigError::Invalid("assets.max_conditions_per_node must be positive".into()));
        }
        if self.max_effects_per_node == 0 {
            return Err(ConfigError::Invalid("assets.max_effects_per_node must be positive".into()));
        }
        if self.max_manifest_bytes == 0 {
            return Err(ConfigError::Invalid("assets.max_manifest_bytes must be positive".into()));
        }
        if self.max_compressed_pack_bytes == 0 {
            return Err(ConfigError::Invalid("assets.max_compressed_pack_bytes must be positive".into()));
        }
        if self.max_uncompressed_pack_bytes == 0 {
            return Err(ConfigError::Invalid(
                "assets.max_uncompressed_pack_bytes must be positive".into(),
            ));
        }
        if self.max_uncompressed_pack_bytes < self.max_compressed_pack_bytes {
            return Err(ConfigError::Invalid(
                "assets.max_uncompressed_pack_bytes must be >= assets.max_compressed_pack_bytes".into(),
            ));
        }
        if self.max_compression_ratio == 0 {
            return Err(ConfigError::Invalid("assets.max_compression_ratio must be positive".into()));
        }
        if self.max_asset_files == 0 {
            return Err(ConfigError::Invalid("assets.max_asset_files must be positive".into()));
        }
        if self.max_single_asset_bytes == 0 {
            return Err(ConfigError::Invalid("assets.max_single_asset_bytes must be positive".into()));
        }
        if self.max_validation_issues == 0 {
            return Err(ConfigError::Invalid("assets.max_validation_issues must be positive".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptModuleConfig {
    #[serde(default)]
    pub catalog_path: PathBuf,
    #[serde(default)]
    pub profile_assets: BTreeMap<crate::prompt::PromptProfile, crate::prompt::AssetRef>,
}

impl Default for PromptModuleConfig {
    fn default() -> Self {
        Self {
            catalog_path: PathBuf::from("prompts"),
            profile_assets: BTreeMap::new(),
        }
    }
}

impl PromptModuleConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.catalog_path.as_os_str().is_empty() {
            return Err(ConfigError::Invalid("prompt.catalog_path must not be empty".into()));
        }
        for (profile, asset) in &self.profile_assets {
            if asset.trim().is_empty() {
                return Err(ConfigError::Invalid(format!(
                    "prompt.profile_assets.{} must not be empty",
                    profile.as_str()
                )));
            }
        }
        Ok(())
    }
}

impl TurnContentLimitsConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.max_story_instructions_bytes == 0 {
            return Err(ConfigError::Invalid(
                "content.max_story_instructions_bytes must be positive".into(),
            ));
        }
        if self.max_story_config_bytes == 0 {
            return Err(ConfigError::Invalid("content.max_story_config_bytes must be positive".into()));
        }
        if self.max_scene_bytes == 0 {
            return Err(ConfigError::Invalid("content.max_scene_bytes must be positive".into()));
        }
        if self.max_summary_bytes == 0 {
            return Err(ConfigError::Invalid("content.max_summary_bytes must be positive".into()));
        }
        if self.max_constraints == 0 {
            return Err(ConfigError::Invalid("content.max_constraints must be positive".into()));
        }
        if self.max_constraint_bytes == 0 {
            return Err(ConfigError::Invalid("content.max_constraint_bytes must be positive".into()));
        }
        if self.max_characters == 0 {
            return Err(ConfigError::Invalid("content.max_characters must be positive".into()));
        }
        if self.max_character_bytes == 0 {
            return Err(ConfigError::Invalid("content.max_character_bytes must be positive".into()));
        }
        if self.max_world_facts == 0 {
            return Err(ConfigError::Invalid("content.max_world_facts must be positive".into()));
        }
        if self.max_world_fact_bytes == 0 {
            return Err(ConfigError::Invalid("content.max_world_fact_bytes must be positive".into()));
        }
        if self.max_recent_turns == 0 {
            return Err(ConfigError::Invalid("content.max_recent_turns must be positive".into()));
        }
        if self.max_recent_turn_bytes == 0 {
            return Err(ConfigError::Invalid("content.max_recent_turn_bytes must be positive".into()));
        }
        if self.max_memories == 0 {
            return Err(ConfigError::Invalid("content.max_memories must be positive".into()));
        }
        if self.max_memory_bytes == 0 {
            return Err(ConfigError::Invalid("content.max_memory_bytes must be positive".into()));
        }
        if self.max_plan_bytes == 0 {
            return Err(ConfigError::Invalid("content.max_plan_bytes must be positive".into()));
        }
        if self.max_retrieval_candidates == 0 {
            return Err(ConfigError::Invalid("content.max_retrieval_candidates must be positive".into()));
        }
        if self.max_retrieved_items == 0 {
            return Err(ConfigError::Invalid("content.max_retrieved_items must be positive".into()));
        }
        if self.max_retrieved_item_bytes == 0 {
            return Err(ConfigError::Invalid("content.max_retrieved_item_bytes must be positive".into()));
        }
        if self.max_retrieved_tokens == 0 {
            return Err(ConfigError::Invalid("content.max_retrieved_tokens must be positive".into()));
        }
        if self.max_character_thoughts == 0 {
            return Err(ConfigError::Invalid("content.max_character_thoughts must be positive".into()));
        }
        if self.max_character_thought_bytes == 0 {
            return Err(ConfigError::Invalid(
                "content.max_character_thought_bytes must be positive".into(),
            ));
        }
        if self.max_proposal_bytes == 0 {
            return Err(ConfigError::Invalid("content.max_proposal_bytes must be positive".into()));
        }
        if self.max_validation_issues == 0 {
            return Err(ConfigError::Invalid("content.max_validation_issues must be positive".into()));
        }
        if self.max_validation_issue_bytes == 0 {
            return Err(ConfigError::Invalid(
                "content.max_validation_issue_bytes must be positive".into(),
            ));
        }
        if self.max_trace_spans == 0 {
            return Err(ConfigError::Invalid("content.max_trace_spans must be positive".into()));
        }
        if self.max_trace_field_bytes == 0 {
            return Err(ConfigError::Invalid("content.max_trace_field_bytes must be positive".into()));
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
            max_retrieval_candidates: default_max_retrieval_candidates(),
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
        if self.trace_content == TraceContentPolicy::RedactedContent
            && !redacted_content_allowed(std::env::var("AISE_ENV").ok().as_deref())
        {
            return Err(ConfigError::Invalid(
                "llm.trace_content=redacted_content requires AISE_ENV=development".into(),
            ));
        }
        Ok(())
    }
}

fn redacted_content_allowed(runtime_env: Option<&str>) -> bool {
    runtime_env == Some("development")
}

#[cfg(test)]
#[path = "tests/config_tests.rs"]
mod tests;

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

fn default_max_retrieval_candidates() -> usize {
    64
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
