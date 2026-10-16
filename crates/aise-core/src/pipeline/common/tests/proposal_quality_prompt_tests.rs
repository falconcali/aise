use super::*;
use crate::pipeline::common::{ProposalCheckCriterion, ProposalQualityPolicy, ProposalScoreCriterion};

#[test]
fn projection_omits_empty_collections() {
    let policy = ProposalQualityPolicy::new(Vec::new(), Vec::new());

    let projected = project_proposal_quality(PipelineStage::Generate, &policy).expect("policy projects");

    assert!(projected.check_criteria.is_empty());
    assert!(projected.score_criteria.is_empty());
}

#[test]
fn projection_preserves_order_and_all_fields() {
    let policy = ProposalQualityPolicy::new(
        vec![
            ProposalCheckCriterion {
                name: "first".to_owned(),
                description: "First requirement.".to_owned(),
            },
            ProposalCheckCriterion {
                name: "second".to_owned(),
                description: "Second requirement.".to_owned(),
            },
        ],
        vec![ProposalScoreCriterion {
            name: "quality".to_owned(),
            description: "Quality dimension.".to_owned(),
            min_score: 0.0,
            max_score: 10.0,
            target_score: 8.0,
            target_guidance: "Use clear causal transitions.".to_owned(),
        }],
    );

    let projected = project_proposal_quality(PipelineStage::Validate, &policy).expect("policy projects");

    assert_eq!(
        projected.check_criteria,
        r#"[{"name":"first","description":"First requirement."},{"name":"second","description":"Second requirement."}]"#
    );
    assert_eq!(
        projected.score_criteria,
        r#"[{"name":"quality","description":"Quality dimension.","min_score":0.0,"max_score":10.0,"target_score":8.0,"target_guidance":"Use clear causal transitions."}]"#
    );
}
