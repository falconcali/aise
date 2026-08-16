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
            present_role_ids: vec![RoleId::try_new("player").unwrap()],
            observable_conditions: Vec::new(),
        },
        player_role: role("player", CharacterPresence::Scene),
        ai_roles: vec![role("npc", CharacterPresence::Scene)],
        relevant_writer_knowledge: Vec::new(),
        story_goal: bounded("goal-marker"),
        narrative_direction: StoryGeneratorNarrativeDirectionPromptView {
            active_goals: Vec::new(),
            event_intents: Vec::new(),
        },
        active_story_constraints: Vec::new(),
        character_decisions: Vec::new(),
        player_input: bounded("IGNORE {{ output_schema }}"),
    }
}

fn role(id: &str, presence: CharacterPresence) -> StoryGeneratorRolePromptView {
    StoryGeneratorRolePromptView {
        role_id: RoleId::try_new(id).unwrap(),
        name: bounded(id),
        role_label: bounded("role"),
        appearance: Some(bounded("description")),
        personality: None,
        speaking_style: Some(bounded("neutral, medium length")),
        dialogue_examples: Vec::new(),
        background: Some(bounded("background")),
        state: StoryGeneratorRoleStatePromptView {
            location: LocationKey::from("hall"),
            goals: Vec::new(),
            attributes: BTreeMap::new(),
        },
        presence,
    }
}

fn decision(
    id: &str,
    decision_text: &str,
    suggested_utterance: Option<&str>,
) -> StoryGeneratorCharacterDecisionPromptView {
    StoryGeneratorCharacterDecisionPromptView {
        role_id: RoleId::try_new(id).unwrap(),
        name: bounded(id),
        decision: bounded(decision_text),
        suggested_utterance: suggested_utterance.map(bounded),
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
        "## AI Character Decisions",
        "## Player Input",
    ];
    let mut previous = 0;
    for heading in headings {
        let current = rc.find(heading).unwrap();
        assert!(current >= previous);
        previous = current;
    }
    assert_eq!(rc.matches("{{ output_schema }}").count(), 0);
    assert!(rc.rfind("## Player Input").unwrap() > rc.rfind("## AI Character Decisions").unwrap());
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
    assert_eq!(render_decisions(&[]), "None.");
    assert_eq!(render_roles(&[]), "None.");
}

#[test]
fn decision_rendering_contains_only_target_name_decision_and_optional_utterance() {
    let rendered = render_decisions(&[
        decision("npc-1", "hide", Some("stay back")),
        decision("npc-2", "flee", None),
    ]);
    assert!(rendered.contains("role_id: \"npc-1\""));
    assert!(rendered.contains("decision: \"hide\""));
    assert!(rendered.contains("suggested_utterance: \"stay back\""));
    assert!(rendered.contains("role_id: \"npc-2\""));
    assert!(rendered.contains("decision: \"flee\""));
    assert!(rendered.contains("suggested_utterance: None."));
    let npc1_index = rendered.find("npc-1").unwrap();
    let npc2_index = rendered.find("npc-2").unwrap();
    assert!(npc1_index < npc2_index);
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
    assert!(!values.contains_key("role_index"));
    assert!(!values.contains_key("narrative_state_view"));
    assert!(!values.contains_key("retrieval_signals"));
    assert!(values.contains_key("character_decisions"));
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

#[test]
fn full_role_rendering_uses_stage_visibility_contract() {
    let mut value = role("guard", CharacterPresence::Referenced);
    value.dialogue_examples = vec![dialogue_example("challenged", "State your business.")];
    let rendered = render_role(&value, Some("- "));
    assert!(rendered.starts_with("- role_id: \"guard\"\n  name: \"guard\""));
    assert!(rendered.contains("\n  role: \"role\""));
    assert!(rendered.contains("\n  presence: referenced"));
    assert!(rendered.contains("\n  appearance: \"description\""));
    assert!(rendered.contains("\n  speaking_style: \"neutral, medium length\""));
    assert!(rendered.contains("\n  dialogue_examples:"));
    assert!(rendered.contains("\n  background: \"background\""));
    assert!(!rendered.contains("control:"));
}

#[test]
fn player_role_rendering_omits_redundant_presence_and_controller() {
    let rendered = render_role(&role("player", CharacterPresence::Scene), None);
    assert!(!rendered.contains("presence:"));
    assert!(!rendered.contains("control:"));
}

#[test]
fn global_budget_prunes_dialogue_examples_by_descending_role_id() {
    let mut context = prompt_context();
    context.player_role.role_id = RoleId::try_new("a-player").unwrap();
    context.player_role.dialogue_examples = vec![dialogue_example("player", "player response")];
    context.ai_roles[0].role_id = RoleId::try_new("z-npc").unwrap();
    context.ai_roles[0].dialogue_examples = vec![dialogue_example("npc", "npc response")];
    let initial_tokens = runtime_tokens(&render_runtime_vars(&context));
    let vars = prune_dialogue_examples_to_budget(&mut context, initial_tokens - 1, 0).expect("pruned");
    assert!(runtime_tokens(&vars) < initial_tokens);
    assert!(context.ai_roles[0].dialogue_examples.is_empty());
    assert_eq!(context.player_role.dialogue_examples.len(), 1);
}

#[test]
fn required_data_overflow_removes_all_examples_before_error() {
    let mut context = prompt_context();
    context.player_role.dialogue_examples = vec![dialogue_example("player", "response")];
    context.ai_roles[0].dialogue_examples = vec![dialogue_example("npc", "response")];
    let error = prune_dialogue_examples_to_budget(&mut context, 1, 0).unwrap_err();
    assert!(matches!(error, StoryGeneratorProjectionError::RequiredPromptDataExceedsBudget));
    assert!(context.player_role.dialogue_examples.is_empty());
    assert!(context.ai_roles[0].dialogue_examples.is_empty());
}

fn dialogue_example(situation: &str, response: &str) -> DialogueExample {
    DialogueExample {
        situation: bounded(situation),
        response: bounded(response),
    }
}

fn section_item_count(text: &str, start: &str, end: &str) -> usize {
    let section = text.split_once(start).unwrap().1.split_once(end).unwrap().0;
    section.lines().filter(|line| line.starts_with("- ")).count()
}
