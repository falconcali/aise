use super::*;

#[test]
fn default_limits_match_plan_contract() {
    let config = PlanConfig::default();

    assert_eq!(config.max_retrieval_candidates_per_kind, 32);
    assert_eq!(config.max_story_goal_bytes, 512);
}

#[test]
fn zero_retrieval_candidate_limit_is_valid() {
    let config: PlanConfig =
        serde_json::from_str(r#"{"max_retrieval_candidates_per_kind":0,"max_story_goal_bytes":512}"#)
            .expect("valid plan config");

    assert_eq!(config.max_retrieval_candidates_per_kind, 0);
}
