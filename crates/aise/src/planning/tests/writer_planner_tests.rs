use super::*;
use crate::core::turn_data::ContextSource;

#[test]
fn parse_plan_reads_requests_and_goal() {
    let plan = parse_plan(
        r#"{"retrieval_requests":[{"query":"the gate","sources":["historical_story","world_knowledge"]}],"character_requests":["c-1","c-2"],"story_goal":{"summary":"reach the gate"}}"#,
    )
    .expect("valid plan");
    assert_eq!(plan.retrieval_requests.len(), 1);
    assert_eq!(plan.retrieval_requests[0].query, "the gate");
    assert_eq!(
        plan.retrieval_requests[0].sources,
        vec![ContextSource::HistoricalStory, ContextSource::WorldKnowledge]
    );
    assert_eq!(plan.character_requests.len(), 2);
    assert_eq!(plan.story_goal.summary, "reach the gate");
}

#[test]
fn parse_plan_drops_empty_and_duplicate_requests() {
    let plan = parse_plan(
        r#"{"retrieval_requests":[{"query":"  ","sources":[]},{"query":"keep me","sources":[]}],"character_requests":["c-1","c-1",""],"story_goal":{}}"#,
    )
    .expect("valid plan");
    assert_eq!(plan.retrieval_requests.len(), 1, "blank query must be dropped");
    assert_eq!(plan.retrieval_requests[0].query, "keep me");
    assert_eq!(plan.character_requests.len(), 1, "duplicate and blank ids must be dropped");
    assert_eq!(plan.character_requests[0].as_str(), "c-1");
}

#[test]
fn parse_plan_bounds_output_sizes() {
    let long_query = "q".repeat(400);
    let long_goal = "g".repeat(600);
    let character_ids: Vec<String> = (0..20).map(|index| format!("c-{index}")).collect();
    let plan = parse_plan(&format!(
        r#"{{"retrieval_requests":[{{"query":"{long_query}","sources":[]}}],"character_requests":{character_ids:?},"story_goal":{{"summary":"{long_goal}"}}}}"#
    ))
    .expect("valid plan");
    assert!(plan.retrieval_requests[0].query.len() <= super::MAX_QUERY_CHARS);
    assert!(plan.story_goal.summary.len() <= super::MAX_GOAL_CHARS);
    assert!(plan.character_requests.len() <= super::MAX_CHARACTER_REQUESTS);
}

#[test]
fn parse_plan_rejects_invalid_json() {
    assert!(parse_plan("not json").is_err());
}
