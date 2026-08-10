use super::*;
use crate::domain::turn::{CharacterThinkRequest, RetrievalAudience, WriterStoryGoal};
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::CharacterId;
use crate::planning::planner_output::PlannerOutput;

#[test]
fn planner_output_reads_goal_gaps_and_character_requests() {
    let output: PlannerOutput = serde_json::from_str(
        r#"{
            "story_goal":{"summary":"reach the gate"},
            "context_gaps":[{
                "audience":"global_writer",
                "knowledge_kinds":["fact"],
                "entities":[],
                "topics":[],
                "query_text":"the gate",
                "reason":"need location lore"
            }],
            "character_think_requests":[{"character_id":"c-1","reason":"present"}]
        }"#,
    )
    .expect("valid planner output");
    assert_eq!(output.story_goal.summary.as_str(), "reach the gate");
    assert_eq!(output.context_gaps.len(), 1);
    assert_eq!(output.character_think_requests.len(), 1);
    assert_eq!(output.character_think_requests[0].character_id.as_str(), "c-1");
}

#[test]
fn planner_output_rejects_provider_and_budget_fields() {
    for payload in [
        r#"{"story_goal":{"summary":"x"},"provider":"entity"}"#,
        r#"{"story_goal":{"summary":"x"},"budget":10}"#,
        r#"{"story_goal":{"summary":"x"},"top_k":3}"#,
        r#"{"story_goal":{"summary":"x"},"retriever":"bm25"}"#,
        r#"{"story_goal":{"summary":"x"},"narrative_plan":{}}"#,
        r#"{"story_goal":{"summary":"x"},"active_constraints":[]}"#,
    ] {
        let err = serde_json::from_str::<PlannerOutput>(payload);
        assert!(err.is_err(), "must reject forbidden field: {payload}");
    }
}

#[test]
fn planner_output_rejects_unknown_fields() {
    let err = serde_json::from_str::<PlannerOutput>(r#"{"story_goal":{"summary":"x"},"extra":1}"#);
    assert!(err.is_err());
}

#[test]
fn writer_story_goal_roundtrips() {
    let goal = WriterStoryGoal {
        summary: BoundedText::try_new("keep moving".into(), "goal", 512).unwrap(),
    };
    let json = serde_json::to_string(&goal).unwrap();
    let parsed: WriterStoryGoal = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.summary.as_str(), "keep moving");
    let request = CharacterThinkRequest {
        character_id: CharacterId::try_new("c-1").unwrap(),
        reason: BoundedText::try_new("present".into(), "reason", 256).unwrap(),
    };
    assert!(matches!(
        RetrievalAudience::Character {
            character_id: request.character_id.clone()
        },
        RetrievalAudience::Character { .. }
    ));
}
