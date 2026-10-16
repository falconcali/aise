use crate::pipeline::baseline::BaselineConfig;
use crate::pipeline::common::{PipelineError, PipelineStage};
use crate::pipeline::generate::GenerateConfig;
use crate::pipeline::plan::PlanConfig;
use crate::pipeline::repair::RepairConfig;
use crate::pipeline::summary::SummaryConfig;
use crate::pipeline::think::ThinkConfig;
use crate::pipeline::validate::ValidateConfig;
use serde::Deserialize;

pub const MAX_REPAIR_ROUNDS_LIMIT: u32 = 8;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationLoopConfig {
    pub max_repair_rounds: u32,
}

impl ValidationLoopConfig {
    pub fn validate(&self) -> Result<(), PipelineError> {
        if self.max_repair_rounds > MAX_REPAIR_ROUNDS_LIMIT {
            return Err(PipelineError::invalid_config(
                PipelineStage::Validate,
                "validation_max_repair_rounds_exceeded",
                format!("max_repair_rounds must not exceed {MAX_REPAIR_ROUNDS_LIMIT}"),
            ));
        }
        Ok(())
    }
}

impl Default for ValidationLoopConfig {
    fn default() -> Self {
        Self { max_repair_rounds: 2 }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PipelineConfig {
    pub baseline: BaselineConfig,
    pub plan: PlanConfig,
    pub think: ThinkConfig,
    pub generate: GenerateConfig,
    pub validate: ValidateConfig,
    pub repair: RepairConfig,
    pub validation_loop: ValidationLoopConfig,
    pub summary: SummaryConfig,
}

impl PipelineConfig {
    pub fn validate(&self) -> Result<(), PipelineError> {
        self.validate.validate()?;
        self.repair.validate()?;
        self.validation_loop.validate()
    }
}

#[cfg(test)]
#[path = "tests/config_tests.rs"]
mod tests;
