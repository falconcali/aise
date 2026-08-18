use super::assets::AssetLimitsConfig;
use super::character_think::CharacterThinkConfig;
use super::content::TurnContentLimitsConfig;
use super::context::ContextPreparationConfig;
use super::coordinator::CoordinatorConfig;
use super::error::ConfigError;
use super::llm::LlmConfig;
use super::narrative::NarrativeConfig;
use super::planner::PlannerConfig;
use super::prompt::PromptModuleConfig;
use super::retrieval::RetrievalConfig;
use super::state_extractor::StateExtractorConfig;
use super::storage::StorageConfig;
use super::turn::TurnConfig;
use crate::persistence::story_history_read_port::StoryHistoryConfig;
use serde::{Deserialize, Serialize};

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
    pub character_think: CharacterThinkConfig,
    #[serde(default)]
    pub context: ContextPreparationConfig,
    #[serde(default)]
    pub planner: PlannerConfig,
    #[serde(default)]
    pub retrieval: RetrievalConfig,
    #[serde(default)]
    pub assets: AssetLimitsConfig,
    #[serde(default)]
    pub prompt: PromptModuleConfig,
    #[serde(default)]
    pub story_history: StoryHistoryConfig,
    #[serde(default)]
    pub state_extractor: StateExtractorConfig,
    #[serde(default)]
    pub narrative: NarrativeConfig,
}

impl AiseConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.llm.validate()?;
        self.storage.validate()?;
        self.turn.validate()?;
        self.coordinator.validate()?;
        self.content.validate()?;
        self.character_think.validate()?;
        self.context.validate()?;
        self.planner.validate()?;
        self.retrieval.validate()?;
        self.assets.validate()?;
        self.prompt.validate()?;
        self.story_history
            .validate()
            .map_err(|error| ConfigError::Invalid(error.into()))?;
        self.state_extractor.validate()?;
        self.narrative.validate()?;
        if self.context.recent_segments_for_signals > self.content.max_recent_segments {
            return Err(ConfigError::Invalid(
                "context.recent_segments_for_signals must be <= content.max_recent_segments".into(),
            ));
        }
        if self.context.recent_segments_for_signals > 2 {
            return Err(ConfigError::Invalid("context.recent_segments_for_signals must be <= 2".into()));
        }
        if self.context.max_relevant_roles > self.content.max_roles {
            return Err(ConfigError::Invalid(
                "context.max_relevant_roles must be <= content.max_roles".into(),
            ));
        }
        if self.context.max_role_index > self.content.max_roles {
            return Err(ConfigError::Invalid(
                "context.max_role_index must be <= content.max_roles".into(),
            ));
        }
        if self.planner.max_character_think_requests > self.turn.max_character_decisions {
            return Err(ConfigError::Invalid(
                "planner.max_character_think_requests must be <= turn.max_character_decisions".into(),
            ));
        }
        if self.character_think.max_total_output_bytes > self.content.max_character_decision_bytes {
            return Err(ConfigError::Invalid(
                "character_think.max_total_output_bytes must be <= content.max_character_decision_bytes".into(),
            ));
        }
        if self.planner.max_reason_bytes > self.character_think.max_thinking_focus_bytes {
            return Err(ConfigError::Invalid(
                "planner.max_reason_bytes must be <= character_think.max_thinking_focus_bytes".into(),
            ));
        }
        if self.state_extractor.max_role_states < self.content.max_roles {
            return Err(ConfigError::Invalid(
                "state_extractor.max_role_states must be >= content.max_roles".into(),
            ));
        }
        if self.state_extractor.max_relationship_states < self.context.max_relationships {
            return Err(ConfigError::Invalid(
                "state_extractor.max_relationship_states must be >= context.max_relationships".into(),
            ));
        }
        if self.state_extractor.max_context_tokens > self.turn.max_input_tokens {
            return Err(ConfigError::Invalid(
                "state_extractor.max_context_tokens must be <= turn.max_input_tokens".into(),
            ));
        }
        if self.state_extractor.max_output_tokens > self.turn.max_output_tokens {
            return Err(ConfigError::Invalid(
                "state_extractor.max_output_tokens must be <= turn.max_output_tokens".into(),
            ));
        }
        if self
            .state_extractor
            .max_context_tokens
            .saturating_add(self.state_extractor.max_output_tokens)
            > self.turn.max_total_tokens
        {
            return Err(ConfigError::Invalid(
                "state_extractor context and output tokens must be <= turn.max_total_tokens".into(),
            ));
        }
        let role_aggregate_bound = self
            .assets
            .max_profile_total_bytes
            .saturating_add(self.assets.max_role_background_bytes)
            .saturating_add(self.assets.max_text_bytes);
        if self.content.max_role_bytes < role_aggregate_bound {
            return Err(ConfigError::Invalid(
                "content.max_role_bytes is smaller than the configured role aggregate bounds".into(),
            ));
        }
        Ok(())
    }
}
