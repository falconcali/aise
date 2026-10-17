use super::*;
use crate::core::{FactChangeOp, FactId, RumorChangeOp, RumorId};
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
            policy: ContentCapturePolicy::FullContent,
            max_field_bytes: 1024,
            max_observation_bytes: 2048,
            detector_overlap_bytes: 0,
        }),
    );
    let trace = session.begin_trace(TraceSpec {
        name: "extract-trace-test",
        input: None,
        metadata: Vec::new(),
        tags: Vec::new(),
    });
    trace.root().begin(ObservationSpec {
        name: "extract",
        kind: ObservationKind::Chain,
        input: None,
        metadata: Vec::new(),
    })
}

#[test]
fn invalid_output_error_preserves_extract_stage_and_code() {
    let error = PipelineError::invalid_output(PipelineStage::Extract, "extract_narrative_missing_condition", "missing");

    let error = observation_error(&error);

    assert_eq!(error.code, "extract_narrative_missing_condition");
    assert_eq!(error.failure_kind, "invalid_output");
    assert_eq!(error.stage.as_deref(), Some("extract"));
}

#[test]
fn world_and_role_memory_observations_begin_and_finish_under_stage() {
    let stage = root_observation();
    let world = begin_extract_world_knowledge(&stage, "Proposal text.");
    let extraction = WorldKnowledgeExtraction {
        facts: Vec::<FactChangeOp>::new(),
        rumors: vec![RumorChangeOp::Remove {
            id: RumorId::try_new("rumor_1").expect("valid rumor id"),
        }],
        roles_for_memory: vec![RoleId::try_new("bai_suzhen").expect("valid role id")],
    };
    finish_extract_world_knowledge(world, &Ok(extraction));

    let role_id = RoleId::try_new("bai_suzhen").expect("valid role id");
    let role_obs = begin_extract_role_memory(&stage, &role_id, "Proposal text.");
    finish_extract_role_memory(role_obs, &Ok(Vec::new()));

    let failed_world = begin_extract_world_knowledge(&stage, "Proposal text.");
    finish_extract_world_knowledge(
        failed_world,
        &Err(PipelineError::invalid_output(
            PipelineStage::Extract,
            "extract_fact_unknown_id",
            format!("unknown fact {}", FactId::try_new("f1").expect("valid id")),
        )),
    );
}
