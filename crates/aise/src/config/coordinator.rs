use super::error::ConfigError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinatorConfig {
    #[serde(default = "default_max_waiters_per_story")]
    pub max_waiters_per_story: usize,
    #[serde(default = "default_max_total_waiters")]
    pub max_total_waiters: usize,
    #[serde(default = "default_idle_timeout_secs")]
    pub idle_timeout_secs: u64,
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            max_waiters_per_story: default_max_waiters_per_story(),
            max_total_waiters: default_max_total_waiters(),
            idle_timeout_secs: default_idle_timeout_secs(),
        }
    }
}

impl CoordinatorConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.max_waiters_per_story == 0 {
            return Err(ConfigError::Invalid(
                "coordinator.max_waiters_per_story must be positive".into(),
            ));
        }
        if self.max_total_waiters == 0 {
            return Err(ConfigError::Invalid("coordinator.max_total_waiters must be positive".into()));
        }
        if self.idle_timeout_secs == 0 {
            return Err(ConfigError::Invalid("coordinator.idle_timeout_secs must be positive".into()));
        }
        Ok(())
    }
}

fn default_max_waiters_per_story() -> usize {
    16
}

fn default_max_total_waiters() -> usize {
    256
}

fn default_idle_timeout_secs() -> u64 {
    300
}
