use super::*;

fn bounded(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", 256).expect("bounded")
}

fn prompt_context() -> CharacterThinkPromptContext {
    CharacterThinkPromptContext {
        target_character: CharacterThinkCharacterPromptView {
            character_id: CharacterId::from("character-a"),
            name: bounded("A"),
            description: Some(bounded("description")),
            personality: vec![bounded("careful")],
            values: vec![bounded("loyalty")],
            fears: vec![bounded("betrayal")],
        },
        current_character_state: CharacterThinkStatePromptView {
            location: Some(bounded("hall")),
            goals: vec![bounded("stay safe")],
            relevant_attributes: Vec::new(),
        },
        story_continuity: CharacterThinkStoryContinuityPromptView {
            story_summary: bounded("summary-marker"),
            recent_story: vec![bounded("recent-one"), bounded("recent-two")],
        },
        current_scene: CharacterThinkScenePromptView {
            location: Some(bounded("hall")),
            time: Some(bounded("night")),
            situation: Some(bounded("a visitor arrives")),
            observable_conditions: Vec::new(),
        },
        relevant_character_knowledge: vec![
            CharacterThinkKnowledgePromptView {
                kind: CharacterThinkKnowledgeKind::Memory,
                content: bounded("memory-marker"),
            },
            CharacterThinkKnowledgePromptView {
                kind: CharacterThinkKnowledgeKind::Rumor,
                content: bounded("rumor-marker"),
            },
        ],
        narrative_character_impulses: Vec::new(),
        thinking_focus: bounded("focus-marker"),
        player_input: bounded("attempt-marker"),
    }
}

#[test]
fn character_thought_schema_has_exact_engine_owned_fields() {
    let schema = character_thought_output_schema(&CharacterThinkConfig::default());
    let properties = schema["properties"].as_object().expect("properties");
    assert_eq!(properties.len(), 4);
    assert!(properties.contains_key("perception"));
    assert!(properties.contains_key("emotion"));
    assert!(properties.contains_key("goal"));
    assert!(properties.contains_key("possible_action"));
    assert!(!properties.contains_key("character_id"));
    assert_eq!(schema["additionalProperties"], Value::Bool(false));
}

#[test]
fn character_think_runtime_vars_keep_semantic_sections_distinct() {
    let vars = render_runtime_vars(&prompt_context());
    let values = vars.as_map();
    assert!(values["story_summary"].as_str().expect("summary").contains("summary-marker"));
    assert!(values["recent_story"].as_str().expect("recent").contains("recent-one"));
    assert!(values["recent_story"].as_str().expect("recent").contains("recent-two"));
    assert!(
        values["relevant_character_knowledge"]
            .as_str()
            .expect("knowledge")
            .contains("kind: memory")
    );
    assert!(
        values["relevant_character_knowledge"]
            .as_str()
            .expect("knowledge")
            .contains("kind: rumor")
    );
    assert_eq!(values["thinking_focus"].as_str(), Some("\"focus-marker\""));
    assert_eq!(values["player_input"].as_str(), Some("\"attempt-marker\""));
    assert!(!values.contains_key("story_goal"));
    assert!(!values.contains_key("narrative_plan"));
    assert!(!values.contains_key("current_perception"));
}

#[test]
fn character_think_empty_collections_render_canonical_none() {
    assert_eq!(render_recent_story(&[]), "None.");
    assert_eq!(render_knowledge(&[]), "None.");
    assert_eq!(render_impulses(&[]), "None.");
    assert_eq!(quoted_list(&[]), "None.");
}
