use super::*;

fn text(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", usize::MAX).expect("test text should fit")
}

fn report() -> ValidationReport {
    ValidationReport {
        proposal_version: 2,
        checks: vec![
            ValidationCheckResult {
                name: "passing_check".to_owned(),
                description: "passes".to_owned(),
                passed: true,
                issue: None,
            },
            ValidationCheckResult {
                name: "failed_check".to_owned(),
                description: "fails".to_owned(),
                passed: false,
                issue: Some(text("check issue")),
            },
        ],
        scores: vec![
            ValidationScoreResult {
                name: "unmet_score".to_owned(),
                description: "unmet".to_owned(),
                min_score: 0.0,
                max_score: 10.0,
                target_score: 8.0,
                score: 7.0,
                issue: Some(text("score issue")),
            },
            ValidationScoreResult {
                name: "met_score".to_owned(),
                description: "met".to_owned(),
                min_score: 0.0,
                max_score: 10.0,
                target_score: 8.0,
                score: 8.0,
                issue: None,
            },
        ],
    }
}

#[test]
fn report_detects_failed_checks_and_unmet_scores() {
    let report = report();

    assert!(!report.all_checks_pass());
    assert!(!report.all_scores_meet_target());
    assert_eq!(report.unmet_score_count(), 1);
}

#[test]
fn failed_issues_preserve_checks_before_scores() {
    let issues = report().failed_issues();

    assert_eq!(issues.len(), 2);
    assert_eq!(issues[0].kind, ValidationCriterionKind::Check);
    assert_eq!(issues[0].name, "failed_check");
    assert_eq!(issues[0].message.as_str(), "check issue");
    assert_eq!(issues[1].kind, ValidationCriterionKind::Score);
    assert_eq!(issues[1].name, "unmet_score");
    assert_eq!(issues[1].message.as_str(), "score issue");
}

#[test]
fn passing_results_contribute_no_issues() {
    let mut report = report();
    report.checks[1].passed = true;
    report.checks[1].issue = None;
    report.scores[0].score = report.scores[0].target_score;
    report.scores[0].issue = None;

    assert!(report.all_checks_pass());
    assert!(report.all_scores_meet_target());
    assert!(report.failed_issues().is_empty());
    assert_eq!(report.unmet_score_count(), 0);
}
