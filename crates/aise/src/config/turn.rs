use super::error::ConfigError;
use serde::{Deserialize, Serialize};

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
    #[serde(default = "default_max_context_tokens")]
    pub max_context_tokens: u64,
    #[serde(default = "default_max_character_decisions")]
    pub max_character_decisions: usize,
    #[serde(default = "default_max_validation_issues")]
    pub max_validation_issues: usize,
    #[serde(default = "default_max_trace_spans")]
    pub max_trace_spans: usize,
    #[serde(default = "default_turn_timeout_ms")]
    pub turn_timeout_ms: u64,
}

impl Default for TurnConfig {
    fn default() -> Self {
        Self {
            max_repair_rounds: 3,
            max_llm_calls: default_max_llm_calls(),
            max_input_tokens: default_max_input_tokens(),
            max_output_tokens: default_max_output_tokens(),
            max_total_tokens: default_max_total_tokens(),
            max_context_tokens: default_max_context_tokens(),
            max_character_decisions: default_max_character_decisions(),
            max_validation_issues: default_max_validation_issues(),
            max_trace_spans: default_max_trace_spans(),
            turn_timeout_ms: default_turn_timeout_ms(),
        }
    }
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
        if self.max_character_decisions == 0 {
            return Err(ConfigError::Invalid("turn.max_character_decisions must be positive".into()));
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

fn default_max_character_decisions() -> usize {
    8
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
