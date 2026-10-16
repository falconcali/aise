use crate::pipeline::common::{PipelineError, PipelineStage};
use crate::pipeline::repair::RepairConfig;

pub(super) fn validate_repaired_proposal(raw: String, config: &RepairConfig) -> Result<String, PipelineError> {
    if raw.trim().is_empty() {
        return Err(PipelineError::EmptyOutput {
            stage: PipelineStage::Repair,
        });
    }
    if raw.len() > config.max_proposal_bytes {
        return Err(PipelineError::invalid_output(
            PipelineStage::Repair,
            "repair_proposal_too_long",
            format!("repair proposal exceeds {} bytes", config.max_proposal_bytes),
        ));
    }
    Ok(raw)
}

#[cfg(test)]
#[path = "tests/repair_output_tests.rs"]
mod tests;
