use super::error::ConfigError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterThinkConfig {
    pub max_input_tokens: u64,
    pub max_output_tokens: u32,
    pub max_thinking_focus_bytes: usize,
    pub max_field_bytes: usize,
    pub max_total_output_bytes: usize,
}

impl Default for CharacterThinkConfig {
    fn default() -> Self {
        Self {
            max_input_tokens: 4096,
            max_output_tokens: 512,
            max_thinking_focus_bytes: 256,
            max_field_bytes: 512,
            max_total_output_bytes: 1024,
        }
    }
}

impl CharacterThinkConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.max_input_tokens == 0 {
            return Err(ConfigError::Invalid("character_think.max_input_tokens must be positive".into()));
        }
        if self.max_output_tokens == 0 {
            return Err(ConfigError::Invalid(
                "character_think.max_output_tokens must be positive".into(),
            ));
        }
        if self.max_thinking_focus_bytes == 0 {
            return Err(ConfigError::Invalid(
                "character_think.max_thinking_focus_bytes must be positive".into(),
            ));
        }
        if self.max_field_bytes == 0 {
            return Err(ConfigError::Invalid("character_think.max_field_bytes must be positive".into()));
        }
        if self.max_total_output_bytes == 0 {
            return Err(ConfigError::Invalid(
                "character_think.max_total_output_bytes must be positive".into(),
            ));
        }
        Ok(())
    }
}
