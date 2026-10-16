use super::*;
use serde_json::json;

fn text(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", usize::MAX).expect("test text should fit")
}

fn check() -> ProposalCheckCriterion {
    ProposalCheckCriterion {
        name: "explicit_constraints".to_owned(),
        description: "Satisfy every explicit constraint.".to_owned(),
    }
}

fn score() -> ProposalScoreCriterion {
    ProposalScoreCriterion {
        name: "narrative_quality".to_owned(),
        description: "Use coherent story prose.".to_owned(),
        min_score: 0.0,
        max_score: 10.0,
        target_score: 8.0,
        target_guidance: "Use clear causal and temporal transitions.".to_owned(),
    }
}

#[test]
fn policy_preserves_configured_criterion_order() {
    let second_check = ProposalCheckCriterion {
        name: "internal_logic".to_owned(),
        description: "Avoid contradictions.".to_owned(),
    };
    let second_score = ProposalScoreCriterion {
        name: "story_goal_completion".to_owned(),
        description: "Advance the immediate goal.".to_owned(),
        min_score: 0.0,
        max_score: 10.0,
        target_score: 8.0,
        target_guidance: "Make causally grounded progress.".to_owned(),
    };
    let policy = ProposalQualityPolicy::new(vec![check(), second_check], vec![score(), second_score]);

    assert_eq!(
        policy
            .checks()
            .iter()
            .map(|criterion| criterion.name.as_str())
            .collect::<Vec<_>>(),
        ["explicit_constraints", "internal_logic"]
    );
    assert_eq!(
        policy
            .scores()
            .iter()
            .map(|criterion| criterion.name.as_str())
            .collect::<Vec<_>>(),
        ["narrative_quality", "story_goal_completion"]
    );
}

#[test]
fn check_issue_serialization_has_no_score_fields() {
    let value = serde_json::to_value(ValidationIssue::Check {
        name: "explicit_constraints".to_owned(),
        message: text("Remove the prohibited outcome."),
    })
    .expect("check issue should serialize");

    assert_eq!(
        value,
        json!({
            "kind": "Check",
            "name": "explicit_constraints",
            "message": "Remove the prohibited outcome."
        })
    );
}

#[test]
fn score_issue_serialization_has_current_and_target_scores() {
    let value = serde_json::to_value(ValidationIssue::Score {
        name: "narrative_quality".to_owned(),
        current_score: 6.5,
        target_score: 8.0,
        message: text("Clarify the causal transition."),
    })
    .expect("score issue should serialize");

    assert_eq!(
        value,
        json!({
            "kind": "Score",
            "name": "narrative_quality",
            "current_score": 6.5,
            "target_score": 8.0,
            "message": "Clarify the causal transition."
        })
    );
}
