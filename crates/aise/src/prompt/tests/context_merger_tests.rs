use super::*;
use crate::core::turn_data::{BaselineContext, CharacterThought, ContextItem, ContextSource, StoryGoal, WriterPlan};
use crate::domain::character::{CharacterState, InternalState};
use crate::domain::ids::CharacterId;
use crate::domain::story_state::StoryConfig;
use crate::llm::message::Role;

fn character(id: &str, name: &str) -> CharacterState {
    CharacterState {
        id: CharacterId::from(id),
        name: name.into(),
        bio: format!("bio of {name}"),
        internal_state: InternalState::default(),
    }
}

fn baseline() -> BaselineContext {
    BaselineContext {
        story_instructions: "keep it short".into(),
        story_config: StoryConfig {
            style: Some("fantasy".into()),
            point_of_view: Some("warm".into()),
            tense: Some("english".into()),
        },
        player_character: None,
        current_scene: Some("in the market".into()),
        relevant_characters: vec![character("c-1", "Mira"), character("c-2", "Tom")],
        recent_story: vec!["first scene".into(), "second scene".into()],
        story_summary: "they reached the city".into(),
        active_constraints: Vec::new(),
    }
}

#[test]
fn plan_messages_render_context_and_input() {
    let merger = ContextMerger;
    let messages = merger.plan_messages(&baseline(), "explore the market");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].role, Role::System);
    assert_eq!(messages[1].role, Role::User);
    let system = &messages[0].content;
    assert!(system.contains("story planner"));
    assert!(system.contains("retrieval_requests"));
    assert!(
        system.contains("character_requests must use the exact ids listed in the \"Characters:\" section"),
        "planner prompt must forbid inventing character ids"
    );
    assert!(
        system.contains("introduce them through the story text instead of requesting a character think"),
        "planner prompt must route new characters through story text"
    );
    let user = &messages[1].content;
    assert!(user.contains("they reached the city"));
    assert!(user.contains("in the market"));
    assert!(user.contains("Mira"));
    assert!(user.contains("explore the market"));
}

#[test]
fn thought_messages_render_character_perspective() {
    let merger = ContextMerger;
    let messages = merger.thought_messages(&character("c-1", "Mira"), "speak to Tom", Some("in the market"));
    assert_eq!(messages.len(), 2);
    assert!(messages[0].content.contains("Mira"));
    assert!(messages[0].content.contains("perception"));
    assert!(messages[1].content.contains("speak to Tom"));
}

#[test]
fn generation_messages_render_full_merged_context() {
    let merger = ContextMerger;
    let plan = WriterPlan {
        retrieval_requests: Vec::new(),
        character_requests: Vec::new(),
        story_goal: StoryGoal {
            summary: "reach the gate".into(),
        },
    };
    let retrieved = vec![ContextItem {
        source: ContextSource::WorldKnowledge,
        content: "the gate is guarded".into(),
        score: 1.0,
    }];
    let thoughts = vec![CharacterThought {
        character_id: CharacterId::from("c-1"),
        perception: "the crowd is loud".into(),
        emotion: "anxious".into(),
        goal: "find Tom".into(),
        possible_action: "push through".into(),
    }];
    let messages = merger.generation_messages(GenerationInput {
        baseline: &baseline(),
        plan: &plan,
        retrieved: &retrieved,
        thoughts: &thoughts,
        player_input: "approach the gate",
        issues: &["fix the pacing".to_string()],
        previous_story: Some("old draft"),
    });
    assert_eq!(messages.len(), 2);
    assert!(messages[0].content.contains("fantasy"));
    assert!(messages[0].content.contains("warm"));
    assert!(messages[0].content.contains("story_text"));
    assert!(messages[0].content.contains("fix the pacing"));
    assert!(
        messages[0].content.contains("\"add_facts\": [{\"text\""),
        "proposal schema must describe world facts as objects, not plain strings"
    );
    assert!(
        messages[0].content.contains("\"summary_change\": {\"text\""),
        "proposal schema must describe summary_change as an object with a text field"
    );
    assert!(
        !messages[0].content.contains("summary_delta"),
        "legacy summary_delta must not appear"
    );
    assert!(
        messages[0]
            .content
            .contains("Never output any of these fields as a plain string"),
        "proposal schema must forbid plain-string output for nested fields"
    );
    assert!(
        messages[0].content.contains(
            "character_changes, memory_changes, and affinity targets may only reference characters from the list"
        ),
        "writer prompt must restrict state changes to known characters while allowing story-text introductions"
    );
    let user = &messages[1].content;
    assert!(user.contains("reach the gate"));
    assert!(user.contains("the gate is guarded"));
    assert!(user.contains("perception="));
    assert!(user.contains("old draft"));
    assert!(user.contains("approach the gate"));
}

#[test]
fn bounded_recent_story_and_characters() {
    let merger = ContextMerger;
    let mut baseline = baseline();
    baseline.recent_story = (0..50)
        .map(|index| format!("scene number {index} with a very long tail"))
        .collect();
    let messages = merger.generation_messages(GenerationInput {
        baseline: &baseline,
        plan: &WriterPlan::default(),
        retrieved: &[],
        thoughts: &[],
        player_input: "input",
        issues: &[],
        previous_story: None,
    });
    let user = &messages[1].content;
    assert!(user.contains("1. scene number 0"));
    assert!(
        !user.contains("scene number 9"),
        "recent story is bounded to the configured item count"
    );
}
