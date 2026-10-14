use super::*;
use crate::pipeline::common::PipelineStage;
use crate::trace::AttributeValue;

#[test]
fn metadata_contains_all_requested_and_resolved_counts() {
    let metrics = RetrievalTraceMetrics {
        facts_requested: 3,
        facts_resolved: 3,
        rumors_requested: 2,
        rumors_resolved: 2,
    };

    assert_eq!(
        retrieval_metadata(&metrics),
        vec![
            Attribute::u64(FACTS_REQUESTED, 3),
            Attribute::u64(FACTS_RESOLVED, 3),
            Attribute::u64(RUMORS_REQUESTED, 2),
            Attribute::u64(RUMORS_RESOLVED, 2),
        ]
    );
}

#[test]
fn metadata_counts_use_numeric_types() {
    let metadata = retrieval_metadata(&RetrievalTraceMetrics::default());

    assert!(
        metadata
            .iter()
            .all(|attribute| matches!(attribute.value, AttributeValue::U64(0)))
    );
}

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
