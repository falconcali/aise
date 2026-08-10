use super::error::ConfigError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnContentLimitsConfig {
    pub max_story_profile_bytes: usize,
    pub max_instance_settings: usize,
    pub max_instance_setting_bytes: usize,
    pub max_scene_bytes: usize,
    pub max_summary_bytes: usize,
    pub max_constraints: usize,
    pub max_constraint_bytes: usize,
    pub max_characters: usize,
    pub max_character_bytes: usize,
    pub max_perception_bytes: usize,
    pub max_recent_segments: usize,
    pub max_recent_segment_bytes: usize,
    pub max_recent_segment_tokens: u64,
    pub max_plan_bytes: usize,
    pub max_character_thought_bytes: usize,
    pub max_proposal_bytes: usize,
    pub max_validation_issue_bytes: usize,
    pub max_trace_field_bytes: usize,
}

impl Default for TurnContentLimitsConfig {
    fn default() -> Self {
        Self {
            max_story_profile_bytes: 4096,
            max_instance_settings: 32,
            max_instance_setting_bytes: 256,
            max_scene_bytes: 8192,
            max_summary_bytes: 4096,
            max_constraints: 16,
            max_constraint_bytes: 512,
            max_characters: 16,
            max_character_bytes: 2048,
            max_perception_bytes: 512,
            max_recent_segments: 20,
            max_recent_segment_bytes: 8192,
            max_recent_segment_tokens: 2048,
            max_plan_bytes: 4096,
            max_character_thought_bytes: 1024,
            max_proposal_bytes: 32 * 1024,
            max_validation_issue_bytes: 500,
            max_trace_field_bytes: 2048,
        }
    }
}

impl TurnContentLimitsConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.max_story_profile_bytes == 0 {
            return Err(ConfigError::Invalid("content.max_story_profile_bytes must be positive".into()));
        }
        if self.max_instance_settings == 0 {
            return Err(ConfigError::Invalid("content.max_instance_settings must be positive".into()));
        }
        if self.max_instance_setting_bytes == 0 {
            return Err(ConfigError::Invalid(
                "content.max_instance_setting_bytes must be positive".into(),
            ));
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
        if self.max_perception_bytes == 0 {
            return Err(ConfigError::Invalid("content.max_perception_bytes must be positive".into()));
        }
        if self.max_recent_segments == 0 {
            return Err(ConfigError::Invalid("content.max_recent_segments must be positive".into()));
        }
        if self.max_recent_segment_bytes == 0 {
            return Err(ConfigError::Invalid("content.max_recent_segment_bytes must be positive".into()));
        }
        if self.max_recent_segment_tokens == 0 {
            return Err(ConfigError::Invalid(
                "content.max_recent_segment_tokens must be positive".into(),
            ));
        }
        if self.max_plan_bytes == 0 {
            return Err(ConfigError::Invalid("content.max_plan_bytes must be positive".into()));
        }
        if self.max_character_thought_bytes == 0 {
            return Err(ConfigError::Invalid(
                "content.max_character_thought_bytes must be positive".into(),
            ));
        }
        if self.max_proposal_bytes == 0 {
            return Err(ConfigError::Invalid("content.max_proposal_bytes must be positive".into()));
        }
        if self.max_validation_issue_bytes == 0 {
            return Err(ConfigError::Invalid(
                "content.max_validation_issue_bytes must be positive".into(),
            ));
        }
        if self.max_trace_field_bytes == 0 {
            return Err(ConfigError::Invalid("content.max_trace_field_bytes must be positive".into()));
        }
        Ok(())
    }
}
