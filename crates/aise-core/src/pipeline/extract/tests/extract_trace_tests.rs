use super::*;
use crate::pipeline::common::PipelineStage;

#[test]
fn invalid_output_error_preserves_extract_stage_and_code() {
    let error = PipelineError::invalid_output(PipelineStage::Extract, "extract_narrative_missing_condition", "missing");

    let error = observation_error(&error);

    assert_eq!(error.code, "extract_narrative_missing_condition");
    assert_eq!(error.failure_kind, "invalid_output");
    assert_eq!(error.stage.as_deref(), Some("extract"));
}
