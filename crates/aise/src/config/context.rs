use super::error::ConfigError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextPreparationConfig {
    pub max_relevant_roles: usize,
    pub max_role_index: usize,
    pub max_dialogue_examples_per_role: usize,
    pub max_dialogue_example_tokens_per_role: u64,
    pub max_relationships: usize,
    pub max_condition_event_keys: usize,
    pub max_condition_fact_values: usize,
}

impl Default for ContextPreparationConfig {
    fn default() -> Self {
        Self {
            max_relevant_roles: 8,
            max_role_index: 16,
            max_dialogue_examples_per_role: 4,
            max_dialogue_example_tokens_per_role: 256,
            max_relationships: 64,
            max_condition_event_keys: 256,
            max_condition_fact_values: 256,
        }
    }
}

impl ContextPreparationConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.max_relevant_roles == 0 {
            return Err(ConfigError::Invalid("context.max_relevant_roles must be positive".into()));
        }
        if self.max_role_index == 0 {
            return Err(ConfigError::Invalid("context.max_role_index must be positive".into()));
        }
        if self.max_dialogue_examples_per_role == 0 {
            return Err(ConfigError::Invalid(
                "context.max_dialogue_examples_per_role must be positive".into(),
            ));
        }
        if self.max_dialogue_example_tokens_per_role == 0 {
            return Err(ConfigError::Invalid(
                "context.max_dialogue_example_tokens_per_role must be positive".into(),
            ));
        }
        if self.max_relationships == 0 {
            return Err(ConfigError::Invalid("context.max_relationships must be positive".into()));
        }
        if self.max_condition_event_keys == 0 {
            return Err(ConfigError::Invalid("context.max_condition_event_keys must be positive".into()));
        }
        if self.max_condition_fact_values == 0 {
            return Err(ConfigError::Invalid(
                "context.max_condition_fact_values must be positive".into(),
            ));
        }
        Ok(())
    }
}
