use super::*;
use crate::pipeline::common::PipelineStage;

#[test]
fn missing_fact_error_preserves_code_and_consistency_failure_kind() {
    let error = PipelineError::inconsistent_data(
        PipelineStage::Retrieval,
        "retrieval_fact_missing",
        "selected fact is missing",
    );

    let observation_error = observation_error(&error);

    assert_eq!(observation_error.code, "retrieval_fact_missing");
    assert_eq!(observation_error.failure_kind, "inconsistent_data");
    assert_eq!(observation_error.stage.as_deref(), Some("retrieval"));
}
