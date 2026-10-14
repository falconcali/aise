use super::*;
use crate::core::tests::snake_pack;
use crate::core::{ActivatedKnowledge, StoryPack};
use crate::pipeline::plan::plan_candidates::{PlanCandidates, collect_candidates};
use serde_json::json;
use std::collections::BTreeMap;

fn candidate_pack() -> StoryPack {
    let mut pack = snake_pack();
    let fact_body = pack.seed_facts.values().next().expect("fixture fact").clone();
    let rumor_body = pack.seed_rumors.values().next().expect("fixture rumor").clone();
    pack.seed_facts = ["fact-a", "fact-b"]
        .into_iter()
        .map(|id| (FactId::try_new(id).expect("valid fact id"), fact_body.clone()))
        .collect::<BTreeMap<_, _>>();
    pack.seed_rumors = ["rumor-a", "rumor-b"]
        .into_iter()
        .map(|id| (RumorId::try_new(id).expect("valid rumor id"), rumor_body.clone()))
        .collect::<BTreeMap<_, _>>();
    pack
}

fn candidates(pack: &StoryPack) -> PlanCandidates<'_> {
    collect_candidates(pack, &ActivatedKnowledge::default(), 8)
}

fn config(max_story_goal_bytes: usize) -> PlanConfig {
    PlanConfig {
        max_retrieval_candidates_per_kind: 8,
        max_story_goal_bytes,
    }
}

fn error_code(error: PipelineError) -> &'static str {
    match error {
        PipelineError::InvalidOutput { code, .. } => code,
        other => panic!("expected invalid output, got {other}"),
    }
}

#[test]
fn valid_json_produces_typed_plan_output() {
    let pack = candidate_pack();
    let raw = json!({
        "story_goal": "Continue the encounter.",
        "facts_to_retrieve": ["fact-a"],
        "rumors_to_retrieve": ["rumor-b"]
    })
    .to_string();

    let output = parse_plan_output(&raw, &candidates(&pack), &config(512)).expect("valid output");

    assert_eq!(output.story_goal.as_str(), "Continue the encounter.");
    assert_eq!(output.facts_to_retrieve[0].as_str(), "fact-a");
    assert_eq!(output.rumors_to_retrieve[0].as_str(), "rumor-b");
}

#[test]
fn prose_outside_json_is_rejected() {
    let pack = candidate_pack();
    let raw = r#"Here is the plan: {"story_goal":"Continue.","facts_to_retrieve":[],"rumors_to_retrieve":[]}"#;

    let error = parse_plan_output(raw, &candidates(&pack), &config(512))
        .err()
        .expect("invalid output");

    assert_eq!(error_code(error), "plan_output_invalid_json");
}

#[test]
fn missing_field_is_rejected() {
    let pack = candidate_pack();
    let raw = r#"{"story_goal":"Continue.","facts_to_retrieve":[]}"#;

    let error = parse_plan_output(raw, &candidates(&pack), &config(512))
        .err()
        .expect("invalid output");

    assert_eq!(error_code(error), "plan_output_invalid_json");
}

#[test]
fn unknown_field_is_rejected() {
    let pack = candidate_pack();
    let raw = r#"{"story_goal":"Continue.","facts_to_retrieve":[],"rumors_to_retrieve":[],"reason":"because"}"#;

    let error = parse_plan_output(raw, &candidates(&pack), &config(512))
        .err()
        .expect("invalid output");

    assert_eq!(error_code(error), "plan_output_invalid_json");
}

#[test]
fn empty_story_goal_is_rejected() {
    let pack = candidate_pack();
    let raw = r#"{"story_goal":" \n ","facts_to_retrieve":[],"rumors_to_retrieve":[]}"#;

    let error = parse_plan_output(raw, &candidates(&pack), &config(512))
        .err()
        .expect("invalid output");

    assert_eq!(error_code(error), "plan_story_goal_empty");
}

#[test]
fn oversized_story_goal_uses_original_utf8_byte_length() {
    let pack = candidate_pack();
    let raw = r#"{"story_goal":"界界","facts_to_retrieve":[],"rumors_to_retrieve":[]}"#;

    let error = parse_plan_output(raw, &candidates(&pack), &config(5))
        .err()
        .expect("invalid output");

    assert_eq!(error_code(error), "plan_story_goal_too_long");
}

#[test]
fn invalid_id_is_rejected_as_invalid_json() {
    let pack = candidate_pack();
    let raw = r#"{"story_goal":"Continue.","facts_to_retrieve":[""],"rumors_to_retrieve":[]}"#;

    let error = parse_plan_output(raw, &candidates(&pack), &config(512))
        .err()
        .expect("invalid output");

    assert_eq!(error_code(error), "plan_output_invalid_json");
}

#[test]
fn unknown_fact_id_is_rejected() {
    let pack = candidate_pack();
    let raw = r#"{"story_goal":"Continue.","facts_to_retrieve":["fact-x"],"rumors_to_retrieve":[]}"#;

    let error = parse_plan_output(raw, &candidates(&pack), &config(512))
        .err()
        .expect("invalid output");

    assert_eq!(error_code(error), "plan_unknown_fact_id");
}

#[test]
fn unknown_rumor_id_is_rejected() {
    let pack = candidate_pack();
    let raw = r#"{"story_goal":"Continue.","facts_to_retrieve":[],"rumors_to_retrieve":["rumor-x"]}"#;

    let error = parse_plan_output(raw, &candidates(&pack), &config(512))
        .err()
        .expect("invalid output");

    assert_eq!(error_code(error), "plan_unknown_rumor_id");
}

#[test]
fn cross_kind_id_is_rejected() {
    let pack = candidate_pack();
    let raw = r#"{"story_goal":"Continue.","facts_to_retrieve":["rumor-a"],"rumors_to_retrieve":[]}"#;

    let error = parse_plan_output(raw, &candidates(&pack), &config(512))
        .err()
        .expect("invalid output");

    assert_eq!(error_code(error), "plan_unknown_fact_id");
}

#[test]
fn duplicate_fact_id_is_rejected() {
    let pack = candidate_pack();
    let raw = r#"{"story_goal":"Continue.","facts_to_retrieve":["fact-a","fact-a"],"rumors_to_retrieve":[]}"#;

    let error = parse_plan_output(raw, &candidates(&pack), &config(512))
        .err()
        .expect("invalid output");

    assert_eq!(error_code(error), "plan_duplicate_fact_id");
}

#[test]
fn duplicate_rumor_id_is_rejected() {
    let pack = candidate_pack();
    let raw = r#"{"story_goal":"Continue.","facts_to_retrieve":[],"rumors_to_retrieve":["rumor-a","rumor-a"]}"#;

    let error = parse_plan_output(raw, &candidates(&pack), &config(512))
        .err()
        .expect("invalid output");

    assert_eq!(error_code(error), "plan_duplicate_rumor_id");
}

#[test]
fn empty_selections_are_valid() {
    let pack = candidate_pack();
    let raw = r#"{"story_goal":"Continue.","facts_to_retrieve":[],"rumors_to_retrieve":[]}"#;

    let output = parse_plan_output(raw, &candidates(&pack), &config(512)).expect("valid output");

    assert!(output.facts_to_retrieve.is_empty());
    assert!(output.rumors_to_retrieve.is_empty());
}

#[test]
fn selection_order_is_preserved() {
    let pack = candidate_pack();
    let raw = r#"{"story_goal":"Continue.","facts_to_retrieve":["fact-b","fact-a"],"rumors_to_retrieve":["rumor-b","rumor-a"]}"#;

    let output = parse_plan_output(raw, &candidates(&pack), &config(512)).expect("valid output");

    assert_eq!(
        output.facts_to_retrieve.iter().map(FactId::as_str).collect::<Vec<_>>(),
        ["fact-b", "fact-a"]
    );
    assert_eq!(
        output.rumors_to_retrieve.iter().map(RumorId::as_str).collect::<Vec<_>>(),
        ["rumor-b", "rumor-a"]
    );
}
