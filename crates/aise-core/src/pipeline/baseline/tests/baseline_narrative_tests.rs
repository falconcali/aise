use super::*;
use crate::core::tests::{snake_pack, story_context};
use crate::core::{NarrativeNodeId, NodeStatus};
use crate::trace::{
    ContentCapture, ContentCapturePolicy, ObservabilityContentConfig, ObservationSession, SessionSpec, Trace, TraceSpec,
};
use serde_json::json;

fn test_trace() -> Trace {
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
    session.begin_trace(TraceSpec {
        name: "baseline-narrative-test",
        input: None,
        metadata: Vec::new(),
        tags: Vec::new(),
    })
}

fn text(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", usize::MAX).expect("test text should fit")
}

fn graph() -> crate::core::NarrativeGraph {
    serde_json::from_value(json!({ "nodes": {
        "arrival": {
            "title": "Arrival",
            "dramatic_focus": "The traveler meets the forest keeper.",
            "activate_when": { "type": "story_started" },
            "complete_when": { "type": "semantic", "condition_id": "identified", "criterion": "visitor identified" },
            "constraints": [{ "forbid": "The ritual truth is revealed.", "window": "while_active" }]
        }
    }}))
    .expect("test graph should deserialize")
}

#[test]
fn pack_without_narrative_projects_only_global_constraints() {
    let mut pack = snake_pack();
    pack.constraints = vec![text("Modern technology appears.")];
    pack.narrative = None;

    let trace = test_trace();

    let output = project_narrative(&pack, &story_context(), trace.root()).expect("projection should succeed");

    assert!(output.directions.is_empty());
    assert_eq!(output.constraints, [text("Modern technology appears.")]);
}

#[test]
fn global_constraints_come_before_active_node_constraints() {
    let mut pack = snake_pack();
    pack.constraints = vec![text("Modern technology appears.")];
    pack.narrative = Some(graph());
    let mut story_ctx = story_context();
    story_ctx.narrative.node_status.insert(
        NarrativeNodeId::try_new("arrival").expect("test node id should be valid"),
        NodeStatus::Active,
    );

    let trace = test_trace();

    let output = project_narrative(&pack, &story_ctx, trace.root()).expect("projection should succeed");

    assert_eq!(
        output.constraints,
        [
            text("Modern technology appears."),
            text("The ritual truth is revealed.")
        ]
    );
    assert_eq!(output.directions.len(), 1);
}
