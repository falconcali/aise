use super::*;
use crate::core::turn_data::{
    BaselineContext, CharacterThought, ContextItem, ContextSource, StoryConfig, StoryGoal, WriterPlan,
};
use crate::domain::character::{CharacterState, InternalState};
use crate::domain::ids::CharacterId;
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
            genre: "fantasy".into(),
            tone: "warm".into(),
            language: "english".into(),
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
