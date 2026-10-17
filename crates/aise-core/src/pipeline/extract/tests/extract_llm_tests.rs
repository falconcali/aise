use super::*;
use crate::core::BoundedText;
use serde_json::json;

fn query(id: &str) -> SemanticQuery {
    SemanticQuery {
        condition_id: NarrativeConditionId::try_new(id).expect("valid condition id"),
        criterion: BoundedText::try_new(format!("criterion for {id}"), "criterion", 256).expect("valid criterion"),
    }
}

fn error_code(error: PipelineError) -> &'static str {
    match error {
        PipelineError::InvalidOutput { code, .. } => code,
        other => panic!("expected invalid output, got {other}"),
    }
}

#[test]
fn valid_evaluations_return_exact_truth_map() {
    let queries = [query("a"), query("b"), query("c")];
    let raw = json!({
        "evaluations": [
            {"condition_id": "c", "truth": "unknown"},
            {"condition_id": "a", "truth": "satisfied"},
            {"condition_id": "b", "truth": "unsatisfied"}
        ]
    })
    .to_string();

    let evaluations = parse_narrative_evaluations(&raw, &queries).expect("valid evaluations");

    assert_eq!(
        evaluations[&NarrativeConditionId::try_new("a").expect("valid id")],
        Truth::Satisfied
    );
    assert_eq!(
        evaluations[&NarrativeConditionId::try_new("b").expect("valid id")],
        Truth::Unsatisfied
    );
    assert_eq!(
        evaluations[&NarrativeConditionId::try_new("c").expect("valid id")],
        Truth::Unknown
    );
}

#[test]
fn malformed_json_uses_invalid_json_code() {
    let error = parse_narrative_evaluations("{", &[query("a")]).expect_err("malformed JSON should fail");

    assert_eq!(error_code(error), "extract_narrative_invalid_json");
}

#[test]
fn invalid_truth_uses_invalid_json_code() {
    let raw = r#"{"evaluations":[{"condition_id":"a","truth":"maybe"}]}"#;

    let error = parse_narrative_evaluations(raw, &[query("a")]).expect_err("invalid truth should fail");

    assert_eq!(error_code(error), "extract_narrative_invalid_json");
}

#[test]
fn duplicate_condition_is_rejected() {
    let raw = r#"{"evaluations":[{"condition_id":"a","truth":"satisfied"},{"condition_id":"a","truth":"unknown"}]}"#;

    let error = parse_narrative_evaluations(raw, &[query("a")]).expect_err("duplicate condition should fail");

    assert_eq!(error_code(error), "extract_narrative_duplicate_condition");
}

#[test]
fn unknown_condition_is_rejected() {
    let raw = r#"{"evaluations":[{"condition_id":"other","truth":"satisfied"}]}"#;

    let error = parse_narrative_evaluations(raw, &[query("a")]).expect_err("unknown condition should fail");

    assert_eq!(error_code(error), "extract_narrative_unknown_condition");
}

#[test]
fn missing_condition_is_rejected() {
    let raw = r#"{"evaluations":[{"condition_id":"a","truth":"satisfied"}]}"#;

    let error = parse_narrative_evaluations(raw, &[query("a"), query("b")]).expect_err("missing condition should fail");

    assert_eq!(error_code(error), "extract_narrative_missing_condition");
}
