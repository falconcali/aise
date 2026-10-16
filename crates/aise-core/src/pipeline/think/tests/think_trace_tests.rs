use super::*;
use crate::core::RoleId;
use crate::pipeline::common::PipelineStage;
use crate::trace::{
    ContentCapture, ContentCapturePolicy, ObservabilityContentConfig, ObservationSession, SessionSpec, TraceSpec,
};

fn root_observation() -> Observation {
    let session = ObservationSession::begin(
        SessionSpec {
            id: None,
            user_id: None,
            metadata: Vec::new(),
        },
        ContentCapture::new(ObservabilityContentConfig {
            policy: ContentCapturePolicy::MetadataOnly,
            max_field_bytes: 1024,
            max_observation_bytes: 2048,
            detector_overlap_bytes: 0,
        }),
    );
    let trace = session.begin_trace(TraceSpec {
        name: "think-trace-test",
        input: None,
        metadata: Vec::new(),
        tags: Vec::new(),
    });
    trace.root().begin(ObservationSpec {
        name: "pipeline",
        kind: ObservationKind::Chain,
        input: None,
        metadata: Vec::new(),
    })
}

#[test]
fn stage_and_role_observations_form_explicit_parent_chain() {
    let pipeline = root_observation();
    let think = begin_think(&pipeline);
    let role_id = RoleId::try_new("target").expect("valid role id");
    let role = begin_role(&think, &role_id);

    finish_role(
        role,
        &Err(PipelineError::invalid_output(
            PipelineStage::Think,
            "think_decision_empty",
            "empty",
        )),
    );
    finish_think(
        think,
        &Err(PipelineError::invalid_output(
            PipelineStage::Think,
            "think_decision_empty",
            "empty",
        )),
    );
}

#[test]
fn think_errors_preserve_stable_code_and_stage() {
    let error = PipelineError::inconsistent_data(PipelineStage::Think, "think_resolved_role_missing", "missing role");

    let observation_error = observation_error(&error);

    assert_eq!(observation_error.code, "think_resolved_role_missing");
    assert_eq!(observation_error.failure_kind, "inconsistent_data");
    assert_eq!(observation_error.stage.as_deref(), Some("think"));
}

#[test]
fn cancellation_and_timeout_have_specific_observation_statuses() {
    let cancelled = PipelineError::Llm {
        stage: PipelineStage::Think,
        source: LlmError::Cancelled,
    };
    let timeout = PipelineError::Llm {
        stage: PipelineStage::Think,
        source: LlmError::Timeout,
    };

    assert_eq!(error_status(&cancelled), ObservationStatus::Cancelled);
    assert_eq!(error_status(&timeout), ObservationStatus::DeadlineExceeded);
}
