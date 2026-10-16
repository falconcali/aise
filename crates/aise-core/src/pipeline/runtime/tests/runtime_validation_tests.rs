use super::*;
use crate::core::BoundedText;
use crate::pipeline::common::{ValidationCheckResult, ValidationReport, ValidationScoreResult};

fn text(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", usize::MAX).expect("valid text")
}

fn score(name: &str, min_score: f64, max_score: f64, target_score: f64, value: f64) -> ValidationScoreResult {
    ValidationScoreResult {
        name: name.to_owned(),
        description: name.to_owned(),
        min_score,
        max_score,
        target_score,
        score: value,
        issue: (value < target_score).then(|| text("improve score")),
    }
}

fn proposal(version: u32, checks_pass: bool, scores: Vec<ValidationScoreResult>) -> ValidatedProposal {
    ValidatedProposal {
        proposal: format!("proposal-{version}"),
        report: ValidationReport {
            proposal_version: version,
            checks: vec![ValidationCheckResult {
                name: "gate".to_owned(),
                description: "gate".to_owned(),
                passed: checks_pass,
                issue: (!checks_pass).then(|| text("fix gate")),
            }],
            scores,
        },
    }
}

#[test]
fn target_gap_mse_normalizes_different_score_ranges() {
    let narrow = proposal(1, true, vec![score("quality", 0.0, 10.0, 8.0, 7.0)]);
    let wide = proposal(2, true, vec![score("quality", 0.0, 100.0, 80.0, 70.0)]);

    assert_eq!(target_gap_mse(&narrow).expect("metric"), target_gap_mse(&wide).expect("metric"));
}

#[test]
fn target_gap_mse_ignores_scores_at_or_above_target() {
    let at_target = proposal(1, true, vec![score("quality", 0.0, 10.0, 8.0, 8.0)]);
    let above_target = proposal(2, true, vec![score("quality", 0.0, 10.0, 8.0, 10.0)]);

    assert_eq!(target_gap_mse(&at_target).expect("metric"), 0.0);
    assert_eq!(target_gap_mse(&above_target).expect("metric"), 0.0);
}

#[test]
fn choice_excludes_failed_check_candidates() {
    let history = vec![
        proposal(1, true, vec![score("quality", 0.0, 10.0, 8.0, 6.0)]),
        proposal(2, false, vec![score("quality", 0.0, 10.0, 8.0, 7.9)]),
    ];

    assert_eq!(select_choice(&history).expect("selection").proposal_version, 1);
}

#[test]
fn choice_prefers_smaller_target_gap_mse() {
    let history = vec![
        proposal(1, true, vec![score("quality", 0.0, 10.0, 8.0, 5.0)]),
        proposal(2, true, vec![score("quality", 0.0, 10.0, 8.0, 7.0)]),
    ];

    assert_eq!(select_choice(&history).expect("selection").proposal_version, 2);
}

#[test]
fn choice_breaks_metric_tie_by_fewer_unmet_scores() {
    let history = vec![
        proposal(
            1,
            true,
            vec![
                score("quality", 0.0, 10.0, 8.0, 3.0),
                score("goal", 0.0, 10.0, 8.0, 3.0),
            ],
        ),
        proposal(2, true, vec![score("quality", 0.0, 10.0, 8.0, 3.0)]),
    ];

    assert_eq!(select_choice(&history).expect("selection").proposal_version, 2);
}

#[test]
fn choice_breaks_full_tie_by_newer_proposal() {
    let history = vec![
        proposal(1, true, vec![score("quality", 0.0, 10.0, 8.0, 7.0)]),
        proposal(2, true, vec![score("quality", 0.0, 10.0, 8.0, 7.0)]),
    ];

    assert_eq!(select_choice(&history).expect("selection").proposal_version, 2);
}

#[test]
fn choice_rejects_history_without_eligible_candidate() {
    let history = vec![proposal(3, false, vec![score("quality", 0.0, 10.0, 8.0, 7.0)])];

    assert!(matches!(
        select_choice(&history),
        Err(PipelineError::ValidationFailed {
            code: "validation_choice_unavailable",
            proposal_version: 3,
            ..
        })
    ));
}

#[test]
fn choice_rejects_empty_score_set() {
    let candidate = proposal(4, true, Vec::new());

    assert!(matches!(
        target_gap_mse(&candidate),
        Err(PipelineError::ValidationFailed {
            code: "validation_choice_scores_empty",
            proposal_version: 4,
            ..
        })
    ));
}
