use super::*;
use crate::pipeline::common::PipelineStage;
use crate::trace::AttributeValue;

#[test]
fn retrieval_metadata_contains_all_candidate_and_selection_metrics() {
    let metrics = PlanTraceMetrics {
        fact_candidates_total: 7,
        fact_candidates_offered: 5,
        rumor_candidates_total: 3,
        rumor_candidates_offered: 3,
        facts_selected: 2,
        rumors_selected: 1,
    };

    assert_eq!(
        retrieval_metadata(&metrics),
        vec![
            Attribute::u64(FACT_CANDIDATES_TOTAL, 7),
            Attribute::u64(FACT_CANDIDATES_OFFERED, 5),
            Attribute::u64(FACT_CANDIDATES_DROPPED, 2),
            Attribute::bool(FACT_CANDIDATES_TRUNCATED, true),
            Attribute::u64(RUMOR_CANDIDATES_TOTAL, 3),
            Attribute::u64(RUMOR_CANDIDATES_OFFERED, 3),
            Attribute::u64(RUMOR_CANDIDATES_DROPPED, 0),
            Attribute::bool(RUMOR_CANDIDATES_TRUNCATED, false),
            Attribute::u64(FACTS_SELECTED, 2),
            Attribute::u64(RUMORS_SELECTED, 1),
        ]
    );
}

#[test]
fn warning_guard_requires_fact_or_rumor_truncation() {
    assert!(!candidates_truncated(&PlanTraceMetrics {
        fact_candidates_total: 2,
        fact_candidates_offered: 2,
        rumor_candidates_total: 1,
        rumor_candidates_offered: 1,
        ..PlanTraceMetrics::default()
    }));
    assert!(candidates_truncated(&PlanTraceMetrics {
        fact_candidates_total: 2,
        fact_candidates_offered: 1,
        ..PlanTraceMetrics::default()
    }));
    assert!(candidates_truncated(&PlanTraceMetrics {
        rumor_candidates_total: 2,
        rumor_candidates_offered: 1,
        ..PlanTraceMetrics::default()
    }));
}

#[test]
fn invalid_output_error_preserves_code_and_failure_kind() {
    let error = PipelineError::invalid_output(PipelineStage::Plan, "plan_unknown_fact_id", "unknown fact");

    let observation_error = observation_error(&error);

    assert_eq!(observation_error.code, "plan_unknown_fact_id");
    assert_eq!(observation_error.failure_kind, "invalid_output");
    assert_eq!(observation_error.stage.as_deref(), Some("plan"));
}

#[test]
fn metadata_values_use_numeric_and_boolean_types() {
    let metadata = retrieval_metadata(&PlanTraceMetrics::default());

    assert!(matches!(metadata[0].value, AttributeValue::U64(0)));
    assert!(matches!(metadata[3].value, AttributeValue::Bool(false)));
}
