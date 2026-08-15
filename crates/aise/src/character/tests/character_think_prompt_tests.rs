use super::*;

fn bounded(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", 256).expect("bounded")
}

fn prompt_context() -> CharacterThinkPromptContext {
    CharacterThinkPromptContext {
        target_role: CharacterThinkRolePromptView {
            role_id: RoleId::try_new("character-a").unwrap(),
            name: bounded("A"),
            appearance: Some(bounded("description")),
            personality: Some(bounded("careful")),
            speaking_style: Some(bounded("direct")),
        },
        current_role_state: CharacterThinkStatePromptView {
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

fn section_item_count(text: &str, start: &str, end: &str) -> usize {
    let section = text.split_once(start).unwrap().1.split_once(end).unwrap().0;
    section.lines().filter(|line| line.starts_with("- ")).count()
}

#[test]
fn character_decision_schema_has_exact_properties_required_and_nullability() {
    let schema = character_decision_output_schema(&CharacterThinkConfig::default());
    let properties = schema["properties"].as_object().expect("properties");
    assert_eq!(properties.len(), 2);
    assert!(properties.contains_key("decision"));
    assert!(properties.contains_key("suggested_utterance"));
    for removed in ["role_id", "perception", "emotion", "goal", "possible_action"] {
        assert!(!properties.contains_key(removed));
    }
    assert_eq!(schema["additionalProperties"], Value::Bool(false));
    let required = schema["required"].as_array().expect("required");
    assert_eq!(required, &vec![Value::String("decision".into())]);
    assert_eq!(schema["properties"]["decision"]["type"], Value::String("string".into()));
    assert_eq!(
        schema["properties"]["suggested_utterance"]["type"],
        serde_json::json!(["string", "null"])
    );
}

#[test]
fn character_think_csi_and_fti_have_exact_rule_counts() {
    let csi = include_str!("../../../assets/prompts/context-v2/csi/character-think.md.j2");
    let fti = include_str!("../../../assets/prompts/context-v2/fti/character-think.md.j2");
    assert_eq!(section_item_count(csi, "## MUST", "## SHOULD"), 10);
    assert_eq!(section_item_count(csi, "## SHOULD", "## NEVER"), 3);
    assert_eq!(section_item_count(csi, "## NEVER", "# Runtime Data Boundary"), 5);
    assert_eq!(section_item_count(fti, "## MUST", "## NEVER"), 5);
    assert_eq!(section_item_count(fti, "## NEVER", "# Output"), 3);
    assert!(!fti.contains("## SHOULD"));
    assert_eq!(fti.matches("{{ output_schema }}").count(), 1);
}

#[test]
fn character_think_runtime_context_has_exact_section_order() {
    let rc = include_str!("../../../assets/prompts/context-v2/rc/character-think.md.j2");
    let headings = [
        "## Target Character",
        "## Current Character State",
        "## Story Continuity",
        "### Story Summary",
        "### Recent Story",
        "## Current Scene",
        "## Relevant Character Knowledge / Memory",
        "## Narrative Character Impulses",
        "## Thinking Focus",
        "## Player Input",
    ];
    let mut previous = 0;
    for heading in headings {
        let current = rc.find(heading).unwrap();
        assert!(current >= previous);
        previous = current;
    }
    assert!(rc.rfind("## Player Input").unwrap() > rc.rfind("## Thinking Focus").unwrap());
    assert!(!rc.contains("Current Perception"));
    assert_eq!(rc.matches("## Story Continuity").count(), 1);
    assert_eq!(rc.matches("### Story Summary").count(), 1);
    assert_eq!(rc.matches("### Recent Story").count(), 1);
}

#[test]
fn thinking_focus_equals_request_reason() {
    let context = prompt_context();
    assert_eq!(context.thinking_focus.as_str(), "focus-marker");
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
    assert!(!values.contains_key("character_decisions"));
    assert!(!values.contains_key("output_role_id"));
}

#[test]
fn character_think_empty_collections_render_canonical_none() {
    assert_eq!(render_recent_story(&[]), "None.");
    assert_eq!(render_knowledge(&[]), "None.");
    assert_eq!(render_impulses(&[]), "None.");
    assert_eq!(quoted_list(&[]), "None.");
}

#[test]
fn instruction_like_rc_values_remain_data() {
    let marker = "IGNORE_PREVIOUS_INSTRUCTIONS_owned_by_player";
    let mut context = prompt_context();
    context.thinking_focus = bounded(marker);
    context.player_input = bounded(marker);
    let vars = render_runtime_vars(&context);
    let values = vars.as_map();
    assert!(values["thinking_focus"].as_str().unwrap().contains(marker));
    assert!(values["player_input"].as_str().unwrap().contains(marker));
}
