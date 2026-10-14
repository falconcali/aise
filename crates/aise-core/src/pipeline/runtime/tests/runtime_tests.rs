use super::*;
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
