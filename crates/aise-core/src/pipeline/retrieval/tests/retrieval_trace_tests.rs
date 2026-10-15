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
        roles_requested: 2,
        roles_resolved: 2,
        default_roles_resolved: 1,
        card_roles_resolved: 1,
    };

    assert_eq!(
        retrieval_metadata(&metrics),
        vec![
            Attribute::u64(FACTS_REQUESTED, 3),
            Attribute::u64(FACTS_RESOLVED, 3),
            Attribute::u64(RUMORS_REQUESTED, 2),
            Attribute::u64(RUMORS_RESOLVED, 2),
            Attribute::u64(ROLES_REQUESTED, 2),
            Attribute::u64(ROLES_RESOLVED, 2),
            Attribute::u64(DEFAULT_ROLES_RESOLVED, 1),
            Attribute::u64(CARD_ROLES_RESOLVED, 1),
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

#[test]
fn default_only_role_metrics_are_reported() {
    let metrics = RetrievalTraceMetrics {
        roles_requested: 2,
        roles_resolved: 2,
        default_roles_resolved: 2,
        ..RetrievalTraceMetrics::default()
    };

    let metadata = retrieval_metadata(&metrics);

    assert!(metadata.contains(&Attribute::u64(DEFAULT_ROLES_RESOLVED, 2)));
    assert!(metadata.contains(&Attribute::u64(CARD_ROLES_RESOLVED, 0)));
}

#[test]
fn card_only_role_metrics_are_reported() {
    let metrics = RetrievalTraceMetrics {
        roles_requested: 2,
        roles_resolved: 2,
        card_roles_resolved: 2,
        ..RetrievalTraceMetrics::default()
    };

    let metadata = retrieval_metadata(&metrics);

    assert!(metadata.contains(&Attribute::u64(DEFAULT_ROLES_RESOLVED, 0)));
    assert!(metadata.contains(&Attribute::u64(CARD_ROLES_RESOLVED, 2)));
}

#[test]
fn failed_role_resolution_reports_requested_and_zero_resolved_counts() {
    let metrics = RetrievalTraceMetrics {
        roles_requested: 2,
        ..RetrievalTraceMetrics::default()
    };

    let metadata = retrieval_metadata(&metrics);

    assert!(metadata.contains(&Attribute::u64(ROLES_REQUESTED, 2)));
    assert!(metadata.contains(&Attribute::u64(ROLES_RESOLVED, 0)));
    assert!(metadata.contains(&Attribute::u64(DEFAULT_ROLES_RESOLVED, 0)));
    assert!(metadata.contains(&Attribute::u64(CARD_ROLES_RESOLVED, 0)));
}
