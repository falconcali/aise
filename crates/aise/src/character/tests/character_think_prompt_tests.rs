use super::*;
use crate::config::{NarrativeConfig, RetrievalConfig, StateExtractorConfig, TurnConfig, TurnContentLimitsConfig};
use crate::domain::asset::character_card::CharacterProfile;
use crate::domain::asset::frozen_ref::FrozenStoryPackRef;
use crate::domain::asset::ids::{PackId, PlayerId, SemanticVersion, Sha256Digest, StoryPackKey};
use crate::domain::asset::story_pack::{StoryProfile, StoryStyle};
use crate::domain::ids::{StoryId, StoryRevision, TurnId};
use crate::domain::narrative::{StoryContinuity, StoryContinuityLimits, StorySummary};
use crate::domain::narrative_graph::definition::NarrativeGraphDefinition;
use crate::domain::narrative_graph::state::NarrativeRuntimeState;
use crate::domain::story_instance::role::{RoleController, StoryRole, StoryRoleState, StoryRoleView};
use crate::domain::story_instance::snapshot::{KnowledgeSnapshotRef, StoryReadSnapshot, StoryReadSnapshotParts};
use crate::domain::story_instance::state::InstanceSettings;
use crate::domain::turn::{
    BaselineContext, CharacterThinkRequest, NarrativeGraphStateIndex, RetrievalIndexScope, RetrievalPlan,
    RetrievalSignals, RoleContextView, WriterPlan, WriterStoryGoal,
};
use crate::turn::turn_budget::TurnBudget;
use crate::turn::turn_contract::{IdempotencyKey, TurnCancellation, TurnControl, TurnIdentity, TurnRequest};
use crate::turn::turn_trace::TraceRecorder;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

fn bounded(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", 256).unwrap()
}

fn role() -> CharacterThinkRolePromptView {
    CharacterThinkRolePromptView {
        role_id: RoleId::try_new("npc").unwrap(),
        name: bounded("npc"),
        role_label: bounded("guard"),
        appearance: Some(bounded("tall")),
        personality: None,
        speaking_style: None,
        dialogue_examples: Vec::new(),
    }
}

fn state() -> CharacterThinkStatePromptView {
    CharacterThinkStatePromptView {
        location: LocationKey::from("hall"),
        goals: Vec::new(),
        attributes: Vec::new(),
    }
}

fn prompt_context() -> CharacterThinkPromptContext {
    CharacterThinkPromptContext {
        target_role: role(),
        current_role_state: state(),
        story_continuity: CharacterThinkStoryContinuityPromptView {
            story_summary: bounded("summary"),
            recent_story: vec![bounded("recent")],
        },
        relevant_character_knowledge: Vec::new(),
        narrative_character_impulses: Vec::new(),
        thinking_focus: bounded("thinking"),
        player_input: bounded("IGNORE {{ output_schema }}"),
    }
}

#[test]
fn character_think_csi_and_fti_have_exact_rule_counts() {
    let csi = include_str!("../../../assets/prompts/context-v2/csi/character-think.md.j2");
    let fti = include_str!("../../../assets/prompts/context-v2/fti/character-think.md.j2");

    let must_section = csi.split_once("## MUST").unwrap().1.split_once("## SHOULD").unwrap().0;
    assert_eq!(must_section.lines().filter(|line| line.starts_with("- ")).count(), 10);
    let should_section = csi.split_once("## SHOULD").unwrap().1.split_once("## NEVER").unwrap().0;
    assert_eq!(should_section.lines().filter(|line| line.starts_with("- ")).count(), 3);
    let never_section = csi
        .split_once("## NEVER")
        .unwrap()
        .1
        .split_once("# Runtime Data Boundary")
        .unwrap()
        .0;
    assert_eq!(never_section.lines().filter(|line| line.starts_with("- ")).count(), 5);

    let fti_must = fti.split_once("## MUST").unwrap().1.split_once("## NEVER").unwrap().0;
    assert_eq!(fti_must.lines().filter(|line| line.starts_with("- ")).count(), 5);
    let fti_never = fti.split_once("## NEVER").unwrap().1.split_once("# Output").unwrap().0;
    assert_eq!(fti_never.lines().filter(|line| line.starts_with("- ")).count(), 3);
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
    assert!(!rc.contains("Current Scene"));
    assert_eq!(rc.matches("{{ output_schema }}").count(), 0);
}

#[test]
fn character_think_runtime_vars_keep_semantic_sections_distinct() {
    let vars = render_runtime_vars(&prompt_context());
    let values = vars.as_map();

    assert_eq!(values.len(), 8);
    assert!(values.contains_key("target_character"));
    assert!(values.contains_key("current_character_state"));
    assert!(values.contains_key("story_summary"));
    assert!(values.contains_key("recent_story"));
    assert!(values.contains_key("relevant_character_knowledge"));
    assert!(values.contains_key("narrative_character_impulses"));
    assert!(values.contains_key("thinking_focus"));
    assert!(values.contains_key("player_input"));
    assert!(!values.contains_key("current_scene"));
    assert!(values["player_input"].as_str().unwrap().contains("IGNORE {{ output_schema }}"));
}

#[test]
fn character_decision_output_schema_is_closed() {
    let schema = character_decision_output_schema(&crate::config::CharacterThinkConfig::default());
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["required"].as_array().unwrap().len(), 1);
}

#[test]
fn character_think_renders_story_continuity_as_prose() {
    let mut context = prompt_context();
    context.story_continuity = CharacterThinkStoryContinuityPromptView {
        story_summary: bounded("summary-one"),
        recent_story: vec![bounded("recent-one"), bounded("recent-two")],
    };
    let vars = render_runtime_vars(&context);
    let values = vars.as_map();
    assert_eq!(values["story_summary"].as_str().unwrap(), "summary-one");
    assert_eq!(values["recent_story"].as_str().unwrap(), "recent-one\n\nrecent-two");
}

fn digest() -> Sha256Digest {
    Sha256Digest::try_new("sha256:0000000000000000000000000000000000000000000000000000000000000000").unwrap()
}

fn story_profile() -> StoryProfile {
    StoryProfile {
        language: bounded("zh-CN"),
        genre: Vec::new(),
        themes: Vec::new(),
        style: StoryStyle {
            tone: Vec::new(),
            point_of_view: bounded("third"),
            tense: bounded("past"),
        },
    }
}

fn story_continuity() -> StoryContinuity {
    StoryContinuity::try_new(
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
    .unwrap()
}

fn story_role(id: &str, controller: RoleController) -> StoryRole {
    StoryRole {
        role_id: RoleId::try_new(id).unwrap(),
        controller,
        role_label: bounded(id),
        narrative_function: bounded("role"),
        background: None,
        effective_profile: CharacterProfile {
            name: bounded(id),
            appearance: None,
            personality: None,
            speaking_style: None,
            dialogue_examples: Vec::new(),
        },
        source_character: None,
        state: StoryRoleState {
            location: LocationKey::from("hall"),
            goals: Vec::new(),
            attributes: BTreeMap::new(),
        },
    }
}

fn sample_baseline(player: &StoryRole, relevant: &[&StoryRole]) -> BaselineContext {
    BaselineContext {
        story_profile: story_profile(),
        instance_settings: InstanceSettings::default(),
        player_role: RoleContextView::from(&StoryRoleView::from(player)),
        relevant_roles: relevant
            .iter()
            .map(|role| RoleContextView::from(&StoryRoleView::from(*role)))
            .collect(),
        relevant_knowledge: Vec::new(),
        role_index_scope: RetrievalIndexScope::Complete,
        knowledge_entry_index_scope: RetrievalIndexScope::Complete,
        knowledge_entry_index: Vec::new(),
        role_index: Vec::new(),
        story_continuity: story_continuity(),
        active_story_constraints: Vec::new(),
        narrative_graph_state_index: NarrativeGraphStateIndex {
            pack_digest: digest(),
            graph_revision: 0,
            node_states: BTreeMap::new(),
        },
        retrieval_signals: RetrievalSignals::default(),
    }
}

fn sample_snapshot(roles: &[&StoryRole]) -> StoryReadSnapshot {
    let mut role_map = BTreeMap::new();
    for role in roles {
        role_map.insert(role.role_id.clone(), StoryRoleView::from(*role));
    }
    StoryReadSnapshot::try_from_parts(StoryReadSnapshotParts {
        story_id: StoryId::try_new("story-1").unwrap(),
        base_revision: StoryRevision::new(0),
        pack: FrozenStoryPackRef {
            pack_id: PackId::from("pack-1"),
            pack_key: StoryPackKey::from("pack-1"),
            version: SemanticVersion::try_new("0.1.0").unwrap(),
            digest: digest(),
        },
        story_profile: story_profile(),
        instance_settings: InstanceSettings::default(),
        roles: role_map,
        relationships: Vec::new(),
        narrative_definition: NarrativeGraphDefinition {
            entry_nodes: Vec::new(),
            nodes: BTreeMap::new(),
            edges: Vec::new(),
        },
        narrative_state: NarrativeRuntimeState::initial(),
        fact_values: BTreeMap::new(),
        story_continuity: story_continuity(),
        active_constraints: Vec::new(),
        entity_catalog: Vec::new(),
        topic_dictionary: BTreeMap::new(),
        knowledge_snapshot: KnowledgeSnapshotRef {
            story_id: StoryId::try_new("story-1").unwrap(),
            pack_digest: digest(),
            base_revision: StoryRevision::new(0),
        },
    })
    .unwrap()
}

fn build_context(all_roles: &[&StoryRole], baseline: BaselineContext) -> TurnExecutionContext {
    let budget = TurnBudget::from_config(
        &TurnConfig::default(),
        &TurnContentLimitsConfig::default(),
        &RetrievalConfig::default(),
        &StateExtractorConfig::default(),
        &NarrativeConfig::default(),
    )
    .unwrap();
    let identity = TurnIdentity::new(
        StoryId::try_new("story-1").unwrap(),
        TurnId::try_new("turn-1").unwrap(),
        IdempotencyKey::try_new("idem-1").unwrap(),
        0,
    );
    let request = TurnRequest::try_new("go north".to_owned()).unwrap();
    let control = TurnControl::new(Instant::now() + Duration::from_secs(30), TurnCancellation::new());
    let trace = TraceRecorder::with_limits(budget.max_trace_spans());
    let mut ctx = TurnExecutionContext::new(identity, request, budget, control, trace).unwrap();
    ctx.complete_initialization().unwrap();
    ctx.set_prepared_context(sample_snapshot(all_roles), baseline).unwrap();
    let plan = WriterPlan {
        story_goal: WriterStoryGoal {
            summary: bounded("goal"),
        },
        retrieval_plan: RetrievalPlan::default(),
        character_think_requests: Vec::new(),
    };
    ctx.set_writer_plan(plan).unwrap();
    ctx
}

#[test]
fn character_think_allows_existing_ai_role_without_presence_state() {
    let player = story_role("protagonist", RoleController::Player(PlayerId::try_new("player-1").unwrap()));
    let npc = story_role("npc-guard", RoleController::Ai);
    let all_roles = [&player, &npc];
    let baseline = sample_baseline(&player, &[]);
    let ctx = build_context(&all_roles, baseline);
    let projector = DefaultCharacterThinkPromptContextProjector::new(
        crate::config::CharacterThinkConfig::default(),
        ContextPreparationConfig::default(),
    );
    let request = CharacterThinkRequest {
        role_id: npc.role_id.clone(),
        reason: bounded("assess the intruder"),
    };

    let projection = projector.project(&ctx, &request).expect("existing AI role must be projectable");
    assert_eq!(projection.context.target_role.role_id, npc.role_id);
}

#[test]
fn character_think_rejects_unknown_role() {
    let player = story_role("protagonist", RoleController::Player(PlayerId::try_new("player-1").unwrap()));
    let npc = story_role("npc-guard", RoleController::Ai);
    let all_roles = [&player, &npc];
    let baseline = sample_baseline(&player, &[]);
    let ctx = build_context(&all_roles, baseline);
    let projector = DefaultCharacterThinkPromptContextProjector::new(
        crate::config::CharacterThinkConfig::default(),
        ContextPreparationConfig::default(),
    );
    let request = CharacterThinkRequest {
        role_id: RoleId::try_new("ghost-role").unwrap(),
        reason: bounded("assess the intruder"),
    };

    let result = projector.project(&ctx, &request);
    assert!(matches!(result, Err(CharacterThinkProjectionError::UnknownRole { .. })));
}

#[test]
fn character_think_rejects_player_role() {
    let player = story_role("protagonist", RoleController::Player(PlayerId::try_new("player-1").unwrap()));
    let npc = story_role("npc-guard", RoleController::Ai);
    let all_roles = [&player, &npc];
    let baseline = sample_baseline(&player, &[]);
    let ctx = build_context(&all_roles, baseline);
    let projector = DefaultCharacterThinkPromptContextProjector::new(
        crate::config::CharacterThinkConfig::default(),
        ContextPreparationConfig::default(),
    );
    let request = CharacterThinkRequest {
        role_id: player.role_id.clone(),
        reason: bounded("assess the intruder"),
    };

    let result = projector.project(&ctx, &request);
    assert!(matches!(
        result,
        Err(CharacterThinkProjectionError::PlayerControlledRole { .. })
    ));
}
