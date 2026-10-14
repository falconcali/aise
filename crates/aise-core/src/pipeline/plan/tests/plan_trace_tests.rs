use super::*;
use crate::pipeline::common::PipelineStage;
use crate::trace::AttributeValue;

#[test]
fn candidate_trace_metadata_reports_fact_truncation() {
    let metrics = PlanTraceMetrics {
        fact_candidates_total: 5,
        fact_candidates_offered: 2,
        rumor_candidates_total: 1,
        rumor_candidates_offered: 1,
        facts_selected: 1,
        rumors_selected: 0,
    };

    assert_eq!(
        candidate_trace_metadata(&metrics),
        vec![
            Attribute::u64(FACT_CANDIDATES_TOTAL, 5),
            Attribute::u64(FACT_CANDIDATES_OFFERED, 2),
            Attribute::u64(FACT_CANDIDATES_DROPPED, 3),
            Attribute::bool(FACT_CANDIDATES_TRUNCATED, true),
            Attribute::u64(RUMOR_CANDIDATES_TOTAL, 1),
            Attribute::u64(RUMOR_CANDIDATES_OFFERED, 1),
            Attribute::u64(RUMOR_CANDIDATES_DROPPED, 0),
            Attribute::bool(RUMOR_CANDIDATES_TRUNCATED, false),
            Attribute::u64(FACTS_SELECTED, 1),
            Attribute::u64(RUMORS_SELECTED, 0),
        ]
    );
}

#[test]
fn candidate_trace_metadata_reports_rumor_truncation() {
    let metrics = PlanTraceMetrics {
        fact_candidates_total: 1,
        fact_candidates_offered: 1,
        rumor_candidates_total: 4,
        rumor_candidates_offered: 2,
        facts_selected: 0,
        rumors_selected: 1,
    };

    let metadata = candidate_trace_metadata(&metrics);

    assert_eq!(metadata[2], Attribute::u64(FACT_CANDIDATES_DROPPED, 0));
    assert_eq!(metadata[3], Attribute::bool(FACT_CANDIDATES_TRUNCATED, false));
    assert_eq!(metadata[6], Attribute::u64(RUMOR_CANDIDATES_DROPPED, 2));
    assert_eq!(metadata[7], Attribute::bool(RUMOR_CANDIDATES_TRUNCATED, true));
    assert_eq!(metadata[9], Attribute::u64(RUMORS_SELECTED, 1));
}

#[test]
fn candidate_trace_metadata_reports_no_truncation() {
    let metrics = PlanTraceMetrics {
        fact_candidates_total: 2,
        fact_candidates_offered: 2,
        rumor_candidates_total: 1,
        rumor_candidates_offered: 1,
        facts_selected: 2,
        rumors_selected: 1,
    };

    let metadata = candidate_trace_metadata(&metrics);

    assert_eq!(metadata[2], Attribute::u64(FACT_CANDIDATES_DROPPED, 0));
    assert_eq!(metadata[3], Attribute::bool(FACT_CANDIDATES_TRUNCATED, false));
    assert_eq!(metadata[6], Attribute::u64(RUMOR_CANDIDATES_DROPPED, 0));
    assert_eq!(metadata[7], Attribute::bool(RUMOR_CANDIDATES_TRUNCATED, false));
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
    let metadata = candidate_trace_metadata(&PlanTraceMetrics::default());

    assert!(matches!(metadata[0].value, AttributeValue::U64(0)));
    assert!(matches!(metadata[3].value, AttributeValue::Bool(false)));
}
