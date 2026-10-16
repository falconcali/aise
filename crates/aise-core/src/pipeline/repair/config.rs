use crate::pipeline::common::{PipelineError, PipelineStage};
use serde::Deserialize;

pub const MAX_REPAIR_PROPOSAL_BYTES_LIMIT: usize = 1024 * 1024;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepairConfig {
    pub max_proposal_bytes: usize,
}

impl RepairConfig {
    pub fn validate(&self) -> Result<(), PipelineError> {
        if !(1..=MAX_REPAIR_PROPOSAL_BYTES_LIMIT).contains(&self.max_proposal_bytes) {
            return Err(PipelineError::invalid_config(
                PipelineStage::Repair,
                "repair_max_proposal_bytes_invalid",
                format!("max_proposal_bytes must be between 1 and {MAX_REPAIR_PROPOSAL_BYTES_LIMIT}"),
            ));
        }
        Ok(())
    }
}

impl Default for RepairConfig {
    fn default() -> Self {
        Self {
            max_proposal_bytes: 64 * 1024,
        }
    }
}

#[cfg(test)]
#[path = "tests/config_tests.rs"]
mod tests;
