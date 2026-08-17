use super::*;
use crate::domain::asset::character_card::CharacterProfile;
use crate::domain::asset::ids::{LocationKey, Sha256Digest};
use crate::domain::asset::story_pack::{StoryProfile, StoryStyle};
use crate::domain::ids::TurnId;
use crate::domain::narrative::{
    StoryContinuity, StoryContinuityLimits, StorySegment, StorySegmentOrigin, StorySummary,
};
use crate::domain::story_instance::role::{RoleController, StoryRoleState};
use crate::domain::story_instance::state::InstanceSettings;
use crate::domain::turn::{NarrativeGraphStateIndex, RetrievalIndexScope, RetrievalSignals};

fn bounded(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", 1024).unwrap()
}

fn role() -> RoleContextView {
    RoleContextView {
        role_id: RoleId::try_new("guard").unwrap(),
        role_label: bounded("Guard Captain"),
        narrative_function: bounded("blocks the gate"),
        background: Some(bounded("secret orders")),
        profile: CharacterProfile {
            name: bounded("Guard"),
            appearance: Some(bounded("scarred")),
            personality: Some(bounded("watchful")),
            speaking_style: Some(bounded("formal")),
            dialogue_examples: Vec::new(),
        },
        state: StoryRoleState {
            location: LocationKey::from("gate"),
            goals: vec![bounded("hold the gate")],
            attributes: BTreeMap::new(),
        },
        controller: RoleController::Ai,
    }
}

#[test]
fn writer_role_rendering_has_exact_profile_and_state_fields() {
    let rendered = render_role(&role(), false);
    assert_eq!(
        rendered,
        "role_id: \"guard\"\nname: \"Guard\"\nrole: \"Guard Captain\"\nappearance: \"scarred\"\npersonality: \"watchful\"\nspeaking_style: \"formal\"\nbackground: \"secret orders\"\nlocation: \"gate\"\ngoals: [\"hold the gate\"]"
    );
    assert!(!rendered.contains("control:"));
    assert!(!rendered.contains("presence:"));
    assert!(!rendered.contains("attributes:"));
}

#[test]
fn writer_role_rendering_omits_absent_and_duplicate_fields() {
    let mut value = role();
    value.role_label = value.profile.name.clone();
    value.background = None;
    value.profile.appearance = None;
    value.profile.personality = None;
    value.profile.speaking_style = None;
    let rendered = render_role(&value, false);
    assert!(!rendered.contains("\nrole:"));
    assert!(!rendered.contains("background:"));
    assert!(!rendered.contains("appearance:"));
    assert!(!rendered.contains("personality:"));
    assert!(!rendered.contains("speaking_style:"));
}

#[test]
fn writer_role_collection_uses_required_indentation() {
    let rendered = render_roles(&[role()]);
    assert!(rendered.starts_with("- role_id: \"guard\"\n  name: \"Guard\""));
    assert!(rendered.contains("\n  goals: [\"hold the gate\"]"));
    assert!(!rendered.contains("presence:"));
    assert!(!rendered.contains("attributes:"));
}

#[test]
fn writer_role_rendering_elides_empty_goals_and_attributes() {
    let mut value = role();
    value.state.goals = Vec::new();
    let rendered = render_role(&value, false);
    assert!(!rendered.contains("goals:"));
    assert!(!rendered.contains("attributes:"));
    assert!(rendered.contains("location: \"gate\""));
}

#[test]
fn writer_planner_assets_preserve_required_rule_counts() {
    let csi = include_str!("../../../assets/prompts/context-v2/csi/writer-planner.md.j2");
    let fti = include_str!("../../../assets/prompts/context-v2/fti/writer-planner.md.j2");
    assert_eq!(section_item_count(csi, "## MUST", "## SHOULD"), 10);
    assert_eq!(section_item_count(csi, "## SHOULD", "## NEVER"), 3);
    assert_eq!(section_item_count(csi, "## NEVER", "# Runtime Data Boundary"), 5);
    assert_eq!(section_item_count(fti, "## MUST", "## NEVER"), 5);
    assert_eq!(section_item_count(fti, "## NEVER", "# Output"), 3);
}

fn section_item_count(text: &str, start: &str, end: &str) -> usize {
    let section = text.split_once(start).unwrap().1.split_once(end).unwrap().0;
    section.lines().filter(|line| line.starts_with("- ")).count()
}

fn digest() -> Sha256Digest {
    Sha256Digest::try_new("sha256:0000000000000000000000000000000000000000000000000000000000000000").unwrap()
}

fn minimal_baseline() -> BaselineContext {
    BaselineContext {
        story_profile: StoryProfile {
            language: bounded("zh-CN"),
            genre: Vec::new(),
            themes: Vec::new(),
            style: StoryStyle {
                tone: Vec::new(),
                point_of_view: bounded("third"),
                tense: bounded("past"),
            },
        },
        instance_settings: InstanceSettings::default(),
        player_role: {
            let mut player = role();
            player.role_id = RoleId::try_new("player").unwrap();
            player
        },
        relevant_roles: vec![role()],
        relevant_knowledge: Vec::new(),
        role_index_scope: RetrievalIndexScope::Complete,
        knowledge_entry_index_scope: RetrievalIndexScope::Complete,
        knowledge_entry_index: Vec::new(),
        role_index: Vec::new(),
        story_continuity: StoryContinuity::try_new(
            StorySummary {
                text: bounded(""),
                summarized_through: None,
            },
            Vec::new(),
            StoryContinuityLimits {
                max_summary_bytes: 256,
                max_recent_segments: 4,
                max_recent_segment_bytes: 128,
                max_recent_segment_tokens: 32,
            },
        )
        .unwrap(),
        active_story_constraints: Vec::new(),
        narrative_graph_state_index: NarrativeGraphStateIndex {
            pack_digest: digest(),
            graph_revision: 0,
            node_states: BTreeMap::new(),
        },
        retrieval_signals: RetrievalSignals::default(),
    }
}

#[test]
fn writer_planner_prompt_uses_relevant_characters_without_presence() {
    let baseline = minimal_baseline();
    let projector = WriterPlannerPromptContextProjector;
    let narrative_plan = NarrativePlan {
        active_nodes: Vec::new(),
        active_directions: Vec::new(),
        world_event_intents: Vec::new(),
        character_impulses: Vec::new(),
        effect_dispositions: Vec::new(),
    };
    let player_input = bounded("go north");
    let projection = projector
        .project(&baseline, &narrative_plan, &player_input, &PlannerConfig::default(), 100_000)
        .expect("writer planner projection");

    let vars = projection.rc_vars.as_map();
    assert!(vars.contains_key("relevant_characters"));
    assert!(!vars.contains_key("scene_characters"));
    assert!(!vars.contains_key("referenced_characters"));
    assert!(!vars.contains_key("current_scene"));
    let relevant_characters = vars["relevant_characters"].as_str().unwrap();
    assert!(!relevant_characters.contains("presence:"));
    assert!(relevant_characters.contains("guard"));
}

#[test]
fn writer_planner_renders_story_continuity_as_prose() {
    let mut baseline = minimal_baseline();
    baseline.story_continuity = StoryContinuity::try_new(
        StorySummary {
            text: bounded("summary-one"),
            summarized_through: Some(crate::domain::StorySequence::try_new(1).unwrap()),
        },
        vec![
            StorySegment {
                sequence: crate::domain::StorySequence::try_new(2).unwrap(),
                origin: StorySegmentOrigin::Opening,
                text: bounded("recent-one"),
            },
            StorySegment {
                sequence: crate::domain::StorySequence::try_new(3).unwrap(),
                origin: StorySegmentOrigin::Turn {
                    turn_id: TurnId::try_new("t1").unwrap(),
                },
                text: bounded("recent-two"),
            },
        ],
        StoryContinuityLimits {
            max_summary_bytes: 256,
            max_recent_segments: 4,
            max_recent_segment_bytes: 128,
            max_recent_segment_tokens: 32,
        },
    )
    .unwrap();
    let projector = WriterPlannerPromptContextProjector;
    let narrative_plan = NarrativePlan {
        active_nodes: Vec::new(),
        active_directions: Vec::new(),
        world_event_intents: Vec::new(),
        character_impulses: Vec::new(),
        effect_dispositions: Vec::new(),
    };
    let player_input = bounded("go north");
    let projection = projector
        .project(&baseline, &narrative_plan, &player_input, &PlannerConfig::default(), 100_000)
        .expect("writer planner projection");
    let vars = projection.rc_vars.as_map();
    assert_eq!(vars["story_summary"].as_str().unwrap(), "summary-one");
    assert_eq!(vars["recent_story"].as_str().unwrap(), "recent-one\n\nrecent-two");
}
