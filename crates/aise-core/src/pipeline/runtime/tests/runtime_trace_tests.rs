use super::*;

#[test]
fn validation_loop_trace_records_hard_gate_exhaustion() {
    let error = PipelineError::ValidationFailed {
        code: "validation_hard_gate_exhausted",
        proposal_version: 3,
        message: "repair budget exhausted".to_owned(),
    };

    let error = observation_error(&error);

    assert_eq!(error.code, "validation_hard_gate_exhausted");
    assert_eq!(error.failure_kind, "validation");
    assert_eq!(error.stage.as_deref(), Some("validate"));
}
