use super::*;
use crate::pipeline::common::PipelineStage;

#[test]
fn invalid_output_error_preserves_code_and_failure_kind() {
    let error = PipelineError::invalid_output(PipelineStage::Plan, "plan_unknown_fact_id", "unknown fact");

    let observation_error = observation_error(&error);

    assert_eq!(observation_error.code, "plan_unknown_fact_id");
    assert_eq!(observation_error.failure_kind, "invalid_output");
    assert_eq!(observation_error.stage.as_deref(), Some("plan"));
}
