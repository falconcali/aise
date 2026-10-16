use super::*;
use crate::pipeline::common::PipelineStage;

#[test]
fn invalid_output_error_preserves_repair_stage_and_code() {
    let error = PipelineError::invalid_output(PipelineStage::Repair, "repair_proposal_too_long", "too long");

    let error = observation_error(&error);

    assert_eq!(error.code, "repair_proposal_too_long");
    assert_eq!(error.failure_kind, "invalid_output");
    assert_eq!(error.stage.as_deref(), Some("repair"));
}
