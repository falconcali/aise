use super::*;
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::RoleId;
use crate::domain::turn::{CharacterThinkRequest, KnowledgeDelivery, WriterStoryGoal};
use crate::planning::writer_planner_prompt::writer_planner_output_schema;

#[test]
fn planner_output_reads_goal_gaps_and_character_requests() {
    let output: PlannerOutput = serde_json::from_str(
        r#"{
            "story_goal":"reach the gate",
            "context_gaps":[{
                "delivery":{"kind":"writer"},
                "target_id":null,
                "query_text":"the gate",
                "reason":"need location lore"
            }],
            "character_think_requests":[{"role_id":"c-1","reason":"present"}]
        }"#,
    )
    .expect("valid planner output");
    assert_eq!(output.story_goal.as_str(), "reach the gate");
    assert_eq!(output.context_gaps.len(), 1);
    assert_eq!(output.character_think_requests.len(), 1);
    assert_eq!(output.character_think_requests[0].role_id.as_str(), "c-1");
}

#[test]
fn planner_output_rejects_provider_and_budget_fields() {
    for payload in [
        r#"{"story_goal":"x","context_gaps":[],"character_think_requests":[],"provider":"entity"}"#,
        r#"{"story_goal":"x","context_gaps":[],"character_think_requests":[],"budget":10}"#,
        r#"{"story_goal":"x","context_gaps":[],"character_think_requests":[],"top_k":3}"#,
        r#"{"story_goal":"x","context_gaps":[],"character_think_requests":[],"retriever":"bm25"}"#,
        r#"{"story_goal":"x","context_gaps":[],"character_think_requests":[],"narrative_plan":{}}"#,
        r#"{"story_goal":"x","context_gaps":[],"character_think_requests":[],"active_constraints":[]}"#,
    ] {
        let err = serde_json::from_str::<PlannerOutput>(payload);
        assert!(err.is_err(), "must reject forbidden field: {payload}");
    }
}

#[test]
fn planner_output_rejects_unknown_fields() {
    let err = serde_json::from_str::<PlannerOutput>(
        r#"{"story_goal":"x","context_gaps":[],"character_think_requests":[],"extra":1}"#,
    );
    assert!(err.is_err());
}

#[test]
fn planner_output_rejects_legacy_character_identity_fields() {
    for payload in [
        r#"{"story_goal":"x","context_gaps":[],"character_think_requests":[{"character_id":"c-1","reason":"why"}]}"#,
        r#"{"story_goal":"x","context_gaps":[],"character_think_requests":[{"role_key":"c-1","reason":"why"}]}"#,
        r#"{"story_goal":"x","context_gaps":[{"delivery":{"kind":"character","character_id":"c-1"},"target_id":null,"query_text":"need","reason":"why"}],"character_think_requests":[]}"#,
    ] {
        assert!(serde_json::from_str::<PlannerOutput>(payload).is_err());
    }
}

#[test]
fn planner_output_requires_non_null_arrays() {
    for payload in [
        r#"{"story_goal":"x","character_think_requests":[]}"#,
        r#"{"story_goal":"x","context_gaps":[]}"#,
        r#"{"story_goal":"x","context_gaps":null,"character_think_requests":[]}"#,
        r#"{"story_goal":"x","context_gaps":[],"character_think_requests":null}"#,
    ] {
        assert!(serde_json::from_str::<PlannerOutput>(payload).is_err());
    }
}

#[test]
fn planner_output_requires_tagged_delivery_shape() {
    for delivery in [
        r#""writer""#,
        r#"{"kind":"unknown"}"#,
        r#"{"kind":"writer","role_id":"c-1"}"#,
        r#"{"kind":"character"}"#,
    ] {
        let payload = format!(
            r#"{{"story_goal":"x","context_gaps":[{{"delivery":{delivery},"target_id":null,"query_text":"need","reason":"why"}}],"character_think_requests":[]}}"#
        );
        assert!(
            serde_json::from_str::<PlannerOutput>(&payload).is_err(),
            "accepted malformed delivery: {delivery}"
        );
    }
}

#[test]
fn writer_planner_schema_requires_contract_fields_and_selector_exclusivity() {
    let schema = writer_planner_output_schema(&crate::config::PlannerConfig::default());
    assert_eq!(
        schema["required"],
        serde_json::json!(["story_goal", "context_gaps", "character_think_requests"])
    );
    assert!(schema["properties"]["context_gaps"]["items"]["oneOf"].is_array());
    assert!(schema["properties"]["context_gaps"]["items"]["properties"]["delivery"]["oneOf"].is_array());
    assert_eq!(
        schema["properties"]["character_think_requests"]["items"]["additionalProperties"],
        false
    );
}

#[test]
fn writer_story_goal_roundtrips() {
    let goal = WriterStoryGoal {
        summary: BoundedText::try_new("keep moving", "goal", 512).unwrap(),
    };
    let json = serde_json::to_string(&goal).unwrap();
    let parsed: WriterStoryGoal = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.summary.as_str(), "keep moving");
    let request = CharacterThinkRequest {
        role_id: RoleId::try_new("c-1").unwrap(),
        reason: BoundedText::try_new("present", "reason", 256).unwrap(),
    };
    assert!(matches!(
        KnowledgeDelivery::Character {
            role_id: request.role_id.clone()
        },
        KnowledgeDelivery::Character { .. }
    ));
}
