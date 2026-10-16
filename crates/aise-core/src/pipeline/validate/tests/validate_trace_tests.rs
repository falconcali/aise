use super::*;
use crate::pipeline::common::PipelineStage;

#[test]
fn invalid_output_error_preserves_validate_stage_and_code() {
    let error = PipelineError::invalid_output(PipelineStage::Validate, "validate_issue_missing", "missing issue");

    let error = observation_error(&error);

    assert_eq!(error.code, "validate_issue_missing");
    assert_eq!(error.failure_kind, "invalid_output");
    assert_eq!(error.stage.as_deref(), Some("validate"));
}
