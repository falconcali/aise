use super::error::ConfigError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannerConfig {
    pub max_context_gaps: usize,
    pub max_character_think_requests: usize,
    pub max_goal_bytes: usize,
    pub max_reason_bytes: usize,
}

impl Default for PlannerConfig {
    fn default() -> Self {
        Self {
            max_context_gaps: 8,
            max_character_think_requests: 8,
            max_goal_bytes: 512,
            max_reason_bytes: 256,
        }
    }
}

impl PlannerConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.max_context_gaps == 0 {
            return Err(ConfigError::Invalid("planner.max_context_gaps must be positive".into()));
        }
        if self.max_character_think_requests == 0 {
            return Err(ConfigError::Invalid(
                "planner.max_character_think_requests must be positive".into(),
            ));
        }
        if self.max_goal_bytes == 0 {
            return Err(ConfigError::Invalid("planner.max_goal_bytes must be positive".into()));
        }
        if self.max_reason_bytes == 0 {
            return Err(ConfigError::Invalid("planner.max_reason_bytes must be positive".into()));
        }
        Ok(())
    }
}
