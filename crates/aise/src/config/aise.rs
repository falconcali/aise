use super::assets::AssetLimitsConfig;
use super::content::TurnContentLimitsConfig;
use super::context::ContextPreparationConfig;
use super::coordinator::CoordinatorConfig;
use super::error::ConfigError;
use super::llm::LlmConfig;
use super::planner::PlannerConfig;
use super::prompt::PromptModuleConfig;
use super::retrieval::RetrievalConfig;
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
}

impl AiseConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.llm.validate()?;
        self.storage.validate()?;
        self.turn.validate()?;
        self.coordinator.validate()?;
        self.content.validate()?;
        self.context.validate()?;
        self.planner.validate()?;
        self.retrieval.validate()?;
        self.assets.validate()?;
        self.prompt.validate()?;
        self.story_history
            .validate()
            .map_err(|error| ConfigError::Invalid(error.into()))?;
        if self.context.recent_segments_for_signals > self.content.max_recent_segments {
            return Err(ConfigError::Invalid(
                "context.recent_segments_for_signals must be <= content.max_recent_segments".into(),
            ));
        }
        if self.context.recent_segments_for_signals > 2 {
            return Err(ConfigError::Invalid("context.recent_segments_for_signals must be <= 2".into()));
        }
        if self.context.max_scene_characters > self.content.max_characters {
            return Err(ConfigError::Invalid(
                "context.max_scene_characters must be <= content.max_characters".into(),
            ));
        }
        if self.context.max_character_index > self.content.max_characters {
            return Err(ConfigError::Invalid(
                "context.max_character_index must be <= content.max_characters".into(),
            ));
        }
        if self.planner.max_character_think_requests > self.turn.max_character_thoughts {
            return Err(ConfigError::Invalid(
                "planner.max_character_think_requests must be <= turn.max_character_thoughts".into(),
            ));
        }
        Ok(())
    }
}
