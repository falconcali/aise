use crate::pipeline::common::PipelineError;
use serde::Deserialize;
use std::num::NonZeroUsize;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractConfig {
    pub max_fact_changes: usize,
    pub max_rumor_changes: usize,
    pub max_roles_for_memory: usize,
    pub max_memory_changes_per_role: usize,
    pub max_memory_concurrency: NonZeroUsize,
}

impl ExtractConfig {
    pub fn validate(&self) -> Result<(), PipelineError> {
        Ok(())
    }
}

impl Default for ExtractConfig {
    fn default() -> Self {
        Self {
            max_fact_changes: 8,
            max_rumor_changes: 8,
            max_roles_for_memory: 4,
            max_memory_changes_per_role: 4,
            max_memory_concurrency: NonZeroUsize::new(4).unwrap_or(NonZeroUsize::MIN),
        }
    }
}

#[cfg(test)]
#[path = "tests/config_tests.rs"]
mod tests;
