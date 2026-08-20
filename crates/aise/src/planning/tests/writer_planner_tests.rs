use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::RoleId;
use crate::domain::turn::{CharacterThinkRequest, KnowledgeDelivery, WriterStoryGoal};
use crate::planning::planner_output::{WriterPlannerOutputDto, writer_planner_output_schema};

#[test]
fn writer_planner_output_reads_goal_gaps_and_character_requests() {
    let output: WriterPlannerOutputDto = serde_json::from_str(
        r#"{
            "story_goal":"reach the gate",
            "writer_context_gaps":[{"target_id":"npc-guard","reason":"need location lore"}],
            "character_context_gaps":[],
            "character_think_requests":[{"role_id":"c-1","reason":"present"}]
        }"#,
    )
    .expect("valid planner output");
    assert_eq!(output.story_goal, "reach the gate");
    assert_eq!(output.writer_context_gaps.len(), 1);
    assert_eq!(output.character_context_gaps.len(), 0);
    assert_eq!(output.character_think_requests.len(), 1);
    assert_eq!(output.character_think_requests[0].role_id, "c-1");
}

#[test]
fn writer_planner_output_rejects_provider_and_budget_fields() {
    for payload in [
        r#"{"story_goal":"x","writer_context_gaps":[],"character_context_gaps":[],"character_think_requests":[],"provider":"entity"}"#,
        r#"{"story_goal":"x","writer_context_gaps":[],"character_context_gaps":[],"character_think_requests":[],"budget":10}"#,
        r#"{"story_goal":"x","writer_context_gaps":[],"character_context_gaps":[],"character_think_requests":[],"top_k":3}"#,
        r#"{"story_goal":"x","writer_context_gaps":[],"character_context_gaps":[],"character_think_requests":[],"retriever":"bm25"}"#,
        r#"{"story_goal":"x","writer_context_gaps":[],"character_context_gaps":[],"character_think_requests":[],"narrative_plan":{}}"#,
        r#"{"story_goal":"x","writer_context_gaps":[],"character_context_gaps":[],"character_think_requests":[],"active_constraints":[]}"#,
    ] {
        let err = serde_json::from_str::<WriterPlannerOutputDto>(payload);
        assert!(err.is_err(), "must reject forbidden field: {payload}");
    }
}

#[test]
fn writer_planner_output_rejects_unknown_fields() {
    let err = serde_json::from_str::<WriterPlannerOutputDto>(
        r#"{"story_goal":"x","writer_context_gaps":[],"character_context_gaps":[],"character_think_requests":[],"extra":1}"#,
    );
    assert!(err.is_err());
}

#[test]
fn writer_planner_output_rejects_legacy_character_identity_fields() {
    for payload in [
        r#"{"story_goal":"x","writer_context_gaps":[],"character_context_gaps":[],"character_think_requests":[{"character_id":"c-1","reason":"why"}]}"#,
        r#"{"story_goal":"x","writer_context_gaps":[],"character_context_gaps":[],"character_think_requests":[{"role_key":"c-1","reason":"why"}]}"#,
        r#"{"story_goal":"x","writer_context_gaps":[],"character_context_gaps":[{"character_id":"c-1","target_id":"t","reason":"why"}],"character_think_requests":[]}"#,
    ] {
        assert!(serde_json::from_str::<WriterPlannerOutputDto>(payload).is_err());
    }
}

#[test]
fn writer_planner_output_requires_non_null_arrays() {
    for payload in [
        r#"{"story_goal":"x","character_context_gaps":[],"character_think_requests":[]}"#,
        r#"{"story_goal":"x","writer_context_gaps":[],"character_think_requests":[]}"#,
        r#"{"story_goal":"x","writer_context_gaps":[],"character_context_gaps":[]}"#,
        r#"{"story_goal":"x","writer_context_gaps":null,"character_context_gaps":[],"character_think_requests":[]}"#,
        r#"{"story_goal":"x","writer_context_gaps":[],"character_context_gaps":null,"character_think_requests":[]}"#,
        r#"{"story_goal":"x","writer_context_gaps":[],"character_context_gaps":[],"character_think_requests":null}"#,
    ] {
        assert!(serde_json::from_str::<WriterPlannerOutputDto>(payload).is_err());
    }
}

#[test]
fn writer_planner_output_rejects_query_text_and_delivery_fields() {
    for payload in [
        r#"{"story_goal":"x","writer_context_gaps":[{"target_id":"t","query_text":"need","reason":"why"}],"character_context_gaps":[],"character_think_requests":[]}"#,
        r#"{"story_goal":"x","writer_context_gaps":[{"delivery":{"kind":"writer"},"target_id":"t","reason":"why"}],"character_context_gaps":[],"character_think_requests":[]}"#,
        r#"{"story_goal":"x","writer_context_gaps":[],"character_context_gaps":[{"role_id":"c-1","target_id":"t","query_text":"need","reason":"why"}],"character_think_requests":[]}"#,
    ] {
        assert!(
            serde_json::from_str::<WriterPlannerOutputDto>(payload).is_err(),
            "must reject: {payload}"
        );
    }
}

#[test]
fn writer_planner_schema_requires_contract_fields_without_delivery_union() {
    let schema = writer_planner_output_schema(&crate::config::PlannerConfig::default());
    assert_eq!(
        schema["required"],
        serde_json::json!([
            "story_goal",
            "writer_context_gaps",
            "character_context_gaps",
            "character_think_requests"
        ])
    );
    assert!(schema["properties"]["writer_context_gaps"]["items"]["properties"]["delivery"].is_null());
    assert!(schema["properties"]["writer_context_gaps"]["items"]["properties"]["query_text"].is_null());
    assert_eq!(
        schema["properties"]["writer_context_gaps"]["items"]["additionalProperties"],
        false
    );
    assert_eq!(
        schema["properties"]["character_context_gaps"]["items"]["additionalProperties"],
        false
    );
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
