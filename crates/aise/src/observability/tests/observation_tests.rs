use super::*;
use crate::observability::{
    ContentCapturePolicy, ObservabilityContentConfig, ObservationKind, ObservationSession, SessionSpec, TraceSpec,
};

#[test]
fn capture_content_uses_full_content_policy() {
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
        name: "test-trace",
        input: None,
        metadata: Vec::new(),
        tags: Vec::new(),
    });
    let observation = trace.begin_observation(ObservationSpec {
        name: "test-observation",
        kind: ObservationKind::Chain,
        input: None,
        metadata: Vec::new(),
    });

    let content = observation.capture_content(&serde_json::json!({"value": "output"}));

    assert_eq!(content.map(|value| value.json), Some("{\"value\":\"output\"}".into()));
}

#[test]
fn capture_content_omits_metadata_only_content() {
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
        name: "test-trace",
        input: None,
        metadata: Vec::new(),
        tags: Vec::new(),
    });
    let observation = trace.begin_observation(ObservationSpec {
        name: "test-observation",
        kind: ObservationKind::Chain,
        input: None,
        metadata: Vec::new(),
    });

    assert!(observation.capture_content(&serde_json::json!({"value": "output"})).is_none());
}
