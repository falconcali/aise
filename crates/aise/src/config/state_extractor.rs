use super::error::ConfigError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateExtractorConfig {
    pub max_context_tokens: u64,
    pub max_output_tokens: u64,
    pub max_role_states: usize,
    pub max_relationship_states: usize,
    pub max_knowledge_changes: usize,
    pub max_goals_per_role: usize,
    pub max_attributes_per_role: usize,
    pub max_entities_per_knowledge: usize,
    pub max_topics_per_knowledge: usize,
    pub max_knowledge_context_items: usize,
    pub max_knowledge_context_tokens: u64,
}

impl Default for StateExtractorConfig {
    fn default() -> Self {
        Self {
            max_context_tokens: 4096,
            max_output_tokens: 2048,
            max_role_states: 16,
            max_relationship_states: 64,
            max_knowledge_changes: 64,
            max_goals_per_role: 16,
            max_attributes_per_role: 64,
            max_entities_per_knowledge: 32,
            max_topics_per_knowledge: 16,
            max_knowledge_context_items: 128,
            max_knowledge_context_tokens: 2048,
        }
    }
}

impl StateExtractorConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.max_context_tokens == 0 {
            return Err(ConfigError::Invalid(
                "state_extractor.max_context_tokens must be positive".into(),
            ));
        }
        if self.max_output_tokens == 0 {
            return Err(ConfigError::Invalid(
                "state_extractor.max_output_tokens must be positive".into(),
            ));
        }
        if self.max_role_states == 0 {
            return Err(ConfigError::Invalid("state_extractor.max_role_states must be positive".into()));
        }
        if self.max_relationship_states == 0 {
            return Err(ConfigError::Invalid(
                "state_extractor.max_relationship_states must be positive".into(),
            ));
        }
        if self.max_knowledge_changes == 0 {
            return Err(ConfigError::Invalid(
                "state_extractor.max_knowledge_changes must be positive".into(),
            ));
        }
        if self.max_goals_per_role == 0 {
            return Err(ConfigError::Invalid(
                "state_extractor.max_goals_per_role must be positive".into(),
            ));
        }
        if self.max_attributes_per_role == 0 {
            return Err(ConfigError::Invalid(
                "state_extractor.max_attributes_per_role must be positive".into(),
            ));
        }
        if self.max_entities_per_knowledge == 0 {
            return Err(ConfigError::Invalid(
                "state_extractor.max_entities_per_knowledge must be positive".into(),
            ));
        }
        if self.max_topics_per_knowledge == 0 {
            return Err(ConfigError::Invalid(
                "state_extractor.max_topics_per_knowledge must be positive".into(),
            ));
        }
        if self.max_knowledge_context_items == 0 {
            return Err(ConfigError::Invalid(
                "state_extractor.max_knowledge_context_items must be positive".into(),
            ));
        }
        if self.max_knowledge_context_tokens == 0 {
            return Err(ConfigError::Invalid(
                "state_extractor.max_knowledge_context_tokens must be positive".into(),
            ));
        }
        Ok(())
    }
}
