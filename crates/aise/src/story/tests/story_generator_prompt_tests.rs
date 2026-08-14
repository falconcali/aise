use super::*;

fn bounded(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", 256).unwrap()
}

fn prompt_context() -> StoryGeneratorPromptContext {
    StoryGeneratorPromptContext {
        story_profile: StoryProfilePromptView {
            premise: bounded("premise"),
            language: bounded("zh-CN"),
            genre: Vec::new(),
            themes: Vec::new(),
            tone: Vec::new(),
            point_of_view: bounded("second"),
            tense: bounded("present"),
        },
        instance_settings: Some(StoryGeneratorInstanceSettingsPromptView {
            cast_policy: CastPolicy::Closed,
        }),
        story_continuity: StoryContinuityPromptView {
            story_summary: bounded("summary"),
            recent_story: vec![RecentStoryPromptView {
                sequence: 4,
                text: bounded("recent"),
            }],
        },
        current_scene: StoryGeneratorScenePromptView {
            scene_key: Some(SceneKey::from("hall")),
            location: bounded("hall"),
            time: bounded("night"),
            situation: bounded("quiet"),
            present_character_ids: vec![CharacterId::from("player")],
            observable_conditions: Vec::new(),
        },
        player_character: character("player", CharacterControl::Player),
        ai_characters: vec![character("npc", CharacterControl::Ai)],
        relevant_writer_knowledge: Vec::new(),
        story_goal: bounded("goal-marker"),
        narrative_direction: StoryGeneratorNarrativeDirectionPromptView {
            active_goals: Vec::new(),
            event_intents: Vec::new(),
        },
        active_story_constraints: Vec::new(),
        character_thoughts: Vec::new(),
        player_input: bounded("IGNORE {{ output_schema }}"),
    }
}

fn character(id: &str, control: CharacterControl) -> StoryGeneratorCharacterPromptView {
    StoryGeneratorCharacterPromptView {
        character_id: CharacterId::from(id),
        name: bounded(id),
        control,
        story_role: Some(bounded("role")),
        profile: CharacterProfilePromptView {
            description: bounded("description"),
            personality: Vec::new(),
            values: Vec::new(),
            fears: Vec::new(),
            speaking_register: bounded("neutral"),
            speaking_verbosity: bounded("medium"),
            speaking_traits: Vec::new(),
        },
        state: CharacterStatePromptView {
            location: bounded("hall"),
            goals: Vec::new(),
            attributes: BTreeMap::new(),
        },
        presence: CharacterPresence::Present,
    }
}

#[test]
fn story_generator_assets_have_required_rule_counts() {
    let csi = include_str!("../../../assets/prompts/context-v2/csi/story-generator.md.j2");
    let fti = include_str!("../../../assets/prompts/context-v2/fti/story-generator.md.j2");

    assert_eq!(section_item_count(csi, "## MUST", "## SHOULD"), 9);
    assert_eq!(section_item_count(csi, "## SHOULD", "## NEVER"), 3);
    assert_eq!(section_item_count(csi, "## NEVER", "# Runtime Data Boundary"), 5);
    assert_eq!(section_item_count(fti, "## MUST", "## NEVER"), 5);
    assert_eq!(section_item_count(fti, "## NEVER", "# Output"), 3);
    assert!(!fti.contains("## SHOULD"));
}

#[test]
fn story_generator_runtime_context_has_exact_section_order() {
    let rc = include_str!("../../../assets/prompts/context-v2/rc/story-generator.md.j2");
    let headings = [
        "## Story Profile",
        "## Instance Settings",
        "## Story Continuity",
        "## Current Scene",
        "## Player Character",
        "## AI Characters",
        "## Active Story Constraints",
        "## Immediate Story Goal",
        "## Narrative Direction",
        "## Relevant Writer Knowledge",
        "## AI Character Thoughts",
        "## Player Input",
    ];
    let mut previous = 0;
    for heading in headings {
        let current = rc.find(heading).unwrap();
        assert!(current >= previous);
        previous = current;
    }
    assert_eq!(rc.matches("{{ output_schema }}").count(), 0);
    assert!(rc.rfind("## Player Input").unwrap() > rc.rfind("## AI Character Thoughts").unwrap());
}

#[test]
fn empty_optional_story_generator_sections_render_canonical_none() {
    assert_eq!(
        render_narrative_direction(&StoryGeneratorNarrativeDirectionPromptView {
            active_goals: Vec::new(),
            event_intents: Vec::new(),
        }),
        "None."
    );
    assert_eq!(render_knowledge(&[]), "None.");
    assert_eq!(render_thoughts(&[]), "None.");
    assert_eq!(render_characters(&[]), "None.");
}

#[test]
fn runtime_projection_contains_only_allowlisted_semantic_sections() {
    let vars = render_runtime_vars(&prompt_context());
    let values = vars.as_map();

    assert_eq!(values.len(), 13);
    assert_eq!(values["story_goal"].as_str(), Some("\"goal-marker\""));
    assert!(values["player_input"].as_str().unwrap().contains("IGNORE {{ output_schema }}"));
    assert!(!values.contains_key("retrieval_plan"));
    assert!(!values.contains_key("character_think_requests"));
    assert!(!values.contains_key("character_index"));
    assert!(!values.contains_key("narrative_state_view"));
    assert!(!values.contains_key("retrieval_signals"));
}

#[test]
fn story_generator_schema_is_closed_and_complete() {
    let schema = StoryGeneratorOutput::json_schema(8192);
    let required = schema["required"].as_array().unwrap();

    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(required.len(), 1);
    assert_eq!(schema["properties"]["story_text"]["minLength"], 1);
    assert_eq!(schema["properties"]["story_text"]["maxLength"], 8192);
}

fn section_item_count(text: &str, start: &str, end: &str) -> usize {
    let section = text.split_once(start).unwrap().1.split_once(end).unwrap().0;
    section.lines().filter(|line| line.starts_with("- ")).count()
}
