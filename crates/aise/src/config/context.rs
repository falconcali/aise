use super::error::ConfigError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextPreparationConfig {
    pub max_scene_roles: usize,
    pub max_role_index: usize,
    pub max_relationships: usize,
    pub max_condition_event_keys: usize,
    pub max_condition_fact_values: usize,
    pub max_entity_catalog: usize,
    pub max_signal_entities: usize,
    pub max_signal_topics: usize,
    pub recent_segments_for_signals: usize,
}

impl Default for ContextPreparationConfig {
    fn default() -> Self {
        Self {
            max_scene_roles: 8,
            max_role_index: 16,
            max_relationships: 64,
            max_condition_event_keys: 256,
            max_condition_fact_values: 256,
            max_entity_catalog: 256,
            max_signal_entities: 32,
            max_signal_topics: 32,
            recent_segments_for_signals: 2,
        }
    }
}

impl ContextPreparationConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.max_scene_roles == 0 {
            return Err(ConfigError::Invalid("context.max_scene_roles must be positive".into()));
        }
        if self.max_role_index == 0 {
            return Err(ConfigError::Invalid("context.max_role_index must be positive".into()));
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
        if self.max_entity_catalog == 0 {
            return Err(ConfigError::Invalid("context.max_entity_catalog must be positive".into()));
        }
        if self.max_signal_entities == 0 {
            return Err(ConfigError::Invalid("context.max_signal_entities must be positive".into()));
        }
        if self.max_signal_topics == 0 {
            return Err(ConfigError::Invalid("context.max_signal_topics must be positive".into()));
        }
        if self.recent_segments_for_signals == 0 {
            return Err(ConfigError::Invalid(
                "context.recent_segments_for_signals must be positive".into(),
            ));
        }
        Ok(())
    }
}
