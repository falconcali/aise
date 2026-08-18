use super::error::ConfigError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateExtractorConfig {
    pub max_context_tokens: u64,
    pub max_output_tokens: u64,
    pub max_new_roles_per_turn: usize,
    pub max_role_states: usize,
    pub max_relationship_states: usize,
    pub max_knowledge_items: usize,
    pub max_goals_per_role: usize,
    pub max_attributes_per_role: usize,
    pub max_role_profile_bytes: usize,
    pub max_cast_policy_violations: usize,
    pub max_knowledge_context_items: usize,
    pub max_knowledge_context_tokens: u64,
}

pub const MAX_NEW_ROLES_PER_TURN_HARD_BOUND: usize = 16;

impl Default for StateExtractorConfig {
    fn default() -> Self {
        Self {
            max_context_tokens: 4096,
            max_output_tokens: 2048,
            max_new_roles_per_turn: 4,
            max_role_states: 16,
            max_relationship_states: 64,
            max_knowledge_items: 32,
            max_goals_per_role: 16,
            max_attributes_per_role: 64,
            max_role_profile_bytes: 512,
            max_cast_policy_violations: 8,
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
        if self.max_new_roles_per_turn == 0 || self.max_new_roles_per_turn > MAX_NEW_ROLES_PER_TURN_HARD_BOUND {
            return Err(ConfigError::Invalid(format!(
                "state_extractor.max_new_roles_per_turn must be within 1..={MAX_NEW_ROLES_PER_TURN_HARD_BOUND}"
            )));
        }
        if self.max_role_states == 0 {
            return Err(ConfigError::Invalid("state_extractor.max_role_states must be positive".into()));
        }
        if self.max_relationship_states == 0 {
            return Err(ConfigError::Invalid(
                "state_extractor.max_relationship_states must be positive".into(),
            ));
        }
        if self.max_knowledge_items == 0 {
            return Err(ConfigError::Invalid(
                "state_extractor.max_knowledge_items must be positive".into(),
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
        if self.max_role_profile_bytes == 0 {
            return Err(ConfigError::Invalid(
                "state_extractor.max_role_profile_bytes must be positive".into(),
            ));
        }
        if self.max_cast_policy_violations == 0 {
            return Err(ConfigError::Invalid(
                "state_extractor.max_cast_policy_violations must be positive".into(),
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

#[cfg(test)]
#[path = "tests/state_extractor_tests.rs"]
mod tests;
