use super::*;
use crate::core::{FactId, RumorId};
use crate::pipeline::validate::ValidateScoreResult;

fn validation(decision: ValidationDecision, scores: &[f32]) -> ValidateOutput {
    ValidateOutput {
        scores: scores
            .iter()
            .map(|score| ValidateScoreResult {
                name: "score".to_owned(),
                description: "score".to_owned(),
                score: *score,
                target_score: 10.0,
                issue: String::new(),
            })
            .collect(),
        decision,
    }
}

#[test]
fn accepted_final_validation_selects_last_proposal() {
    let validations = vec![
        validation(ValidationDecision::Repair, &[9.0]),
        validation(ValidationDecision::Accept, &[1.0]),
    ];

    assert_eq!(select_final_index(&validations), 1);
}

#[test]
fn rejected_final_validation_selects_best_scored_proposal() {
    let validations = vec![
        validation(ValidationDecision::Repair, &[3.0, 4.0]),
        validation(ValidationDecision::Repair, &[9.0, 4.0]),
        validation(ValidationDecision::Reject, &[1.0, 1.0]),
    ];

    assert_eq!(select_final_index(&validations), 1);
}

#[test]
fn proposal_version_counts_proposals() {
    let proposals = vec!["a".to_owned(), "b".to_owned()];

    assert_eq!(proposal_version(&proposals), 2);
}

fn plan_output(facts_to_retrieve: Vec<FactId>, rumors_to_retrieve: Vec<RumorId>) -> PlanOutput {
    PlanOutput {
        story_goal: BoundedText::default(),
        facts_to_retrieve,
        rumors_to_retrieve,
    }
}

#[test]
fn empty_plan_selection_skips_retrieval() {
    let output = plan_output(Vec::new(), Vec::new());

    assert!(retrieval_input(&output).is_none());
}

#[test]
fn fact_only_plan_selection_routes_fact_ids_to_retrieval() {
    let fact_ids = vec![FactId::try_new("fact-1").expect("valid fact id")];
    let output = plan_output(fact_ids.clone(), Vec::new());

    let input = retrieval_input(&output).expect("retrieval required");

    assert_eq!(input.fact_ids, fact_ids);
    assert!(input.rumor_ids.is_empty());
}

#[test]
fn rumor_only_plan_selection_routes_rumor_ids_to_retrieval() {
    let rumor_ids = vec![RumorId::try_new("rumor-1").expect("valid rumor id")];
    let output = plan_output(Vec::new(), rumor_ids.clone());

    let input = retrieval_input(&output).expect("retrieval required");

    assert!(input.fact_ids.is_empty());
    assert_eq!(input.rumor_ids, rumor_ids);
}

#[test]
fn mixed_plan_selection_routes_both_id_lists_to_retrieval() {
    let fact_ids = vec![FactId::try_new("fact-1").expect("valid fact id")];
    let rumor_ids = vec![RumorId::try_new("rumor-1").expect("valid rumor id")];
    let output = plan_output(fact_ids.clone(), rumor_ids.clone());

    let input = retrieval_input(&output).expect("retrieval required");

    assert_eq!(input.fact_ids, fact_ids);
    assert_eq!(input.rumor_ids, rumor_ids);
}
