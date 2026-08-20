use super::*;
use crate::config::{NarrativeConfig, RetrievalConfig, StateExtractorConfig, TurnConfig, TurnContentLimitsConfig};
use crate::domain::asset::character_card::CharacterProfile;
use crate::domain::asset::frozen_ref::FrozenStoryPackRef;
use crate::domain::asset::ids::{PackId, PlayerId, SemanticVersion, Sha256Digest, StoryPackKey};
use crate::domain::asset::story_pack::{StoryProfile, StoryStyle};
use crate::domain::ids::{StoryId, StoryRevision, TurnKey, TurnNumber};
use crate::domain::narrative::{StoryContinuity, StoryContinuityLimits, StorySummary};
use crate::domain::narrative_graph::definition::NarrativeGraphDefinition;
use crate::domain::narrative_graph::state::NarrativeRuntimeState;
use crate::domain::story_instance::role::{RoleController, StoryRole, StoryRoleState, StoryRoleView};
use crate::domain::story_instance::snapshot::{KnowledgeSnapshotRef, StoryReadSnapshot, StoryReadSnapshotParts};
use crate::domain::story_instance::state::InstanceSettings;
use crate::domain::turn::{
    BaselineContext, CharacterThinkRequest, NarrativeGraphStateIndex, RetrievalPlan, RetrievalSignals, RoleContextView,
    WriterPlan, WriterStoryGoal,
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
        knowledge: crate::prompt::RoleKnowledgePromptView::default(),
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
        narrative_character_impulses: Vec::new(),
        thinking_focus: bounded("thinking"),
        player_contribution: bounded("IGNORE {{ output_schema }}"),
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
fn character_think_assets_enforce_perceptibility_and_private_thought_boundary() {
    let csi = include_str!("../../../assets/prompts/context-v2/csi/character-think.md.j2");
    let fti = include_str!("../../../assets/prompts/context-v2/fti/character-think.md.j2");
    let mut context = prompt_context();
    context.player_contribution = bounded("我后退一步，问“你是谁”，心想他可能认识我");
    let rendered = render_runtime_vars(&context);
    assert!(
        rendered.as_map()["player_contribution"]
            .as_str()
            .unwrap()
            .contains("心想他可能认识我")
    );
    assert!(csi.contains("only when the Target Character could perceive it as it occurs"));
    assert!(
        csi.contains("never use a private Player Character thought or desired external outcome as character knowledge")
    );
    assert!(csi.contains("never expose a private Player Character thought to the Target Character"));
    assert!(fti.contains("Use only externally perceptible parts of Pending Player Contribution"));
    assert!(fti.contains("private Player Character thoughts as Target Character knowledge"));
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
        "## Narrative Character Impulses",
        "## Thinking Focus",
        "## Pending Player Contribution",
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

    assert_eq!(values.len(), 7);
    assert!(values.contains_key("target_character"));
    assert!(values.contains_key("current_character_state"));
    assert!(values.contains_key("story_summary"));
    assert!(values.contains_key("recent_story"));
    assert!(values.contains_key("narrative_character_impulses"));
    assert!(values.contains_key("thinking_focus"));
    assert!(values.contains_key("player_contribution"));
    assert!(!values.contains_key("current_scene"));
    assert!(
        values["player_contribution"]
            .as_str()
            .unwrap()
            .contains("IGNORE {{ output_schema }}")
    );
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
        story_title: bounded("Untitled Story"),
        story_profile: story_profile(),
        instance_settings: InstanceSettings::default(),
        player_role: RoleContextView::from(&StoryRoleView::from(player)),
        relevant_roles: relevant
            .iter()
            .map(|role| RoleContextView::from(&StoryRoleView::from(*role)))
            .collect(),
        relevant_world_knowledge: crate::domain::turn::RelevantWorldKnowledge::default(),
        knowledge_index: Vec::new(),
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
        story_title: bounded("Untitled Story"),
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
            knowledge_id_high_water: crate::domain::knowledge::KnowledgeIdHighWater::zero(),
        },
        role_id_high_water: crate::domain::ids::RoleIdHighWater::zero(),
    })
    .unwrap()
}

fn build_context(all_roles: &[&StoryRole], baseline: BaselineContext) -> TurnExecutionContext {
    build_context_with_impulses(all_roles, baseline, Vec::new())
}

fn build_context_with_impulses(
    all_roles: &[&StoryRole],
    baseline: BaselineContext,
    character_impulses: Vec<crate::domain::narrative_graph::effect::CharacterImpulse>,
) -> TurnExecutionContext {
    let budget = TurnBudget::from_config(
        &TurnConfig::default(),
        &TurnContentLimitsConfig::default(),
        &RetrievalConfig::default(),
        &StateExtractorConfig::default(),
        &NarrativeConfig::default(),
    )
    .unwrap();
    let identity = TurnIdentity::new(
        TurnKey::new(StoryId::try_new("story-1").unwrap(), TurnNumber::try_new(1).unwrap()),
        IdempotencyKey::try_new("idem-1").unwrap(),
        0,
    );
    let request = TurnRequest::try_new("go north".to_owned()).unwrap();
    let control = TurnControl::new(Instant::now() + Duration::from_secs(30), TurnCancellation::new());
    let trace = TraceRecorder::with_limits(budget.max_trace_spans());
    let mut ctx = TurnExecutionContext::new(identity, request, budget, control, trace).unwrap();
    ctx.complete_initialization().unwrap();
    ctx.set_prepared_context(sample_snapshot(all_roles), baseline).unwrap();
    if !character_impulses.is_empty() {
        let mut plan = crate::domain::narrative_graph::projector::NarrativePlan::empty();
        plan.character_impulses = character_impulses;
        ctx.set_narrative_projection(crate::domain::narrative_graph::projector::NarrativeProjection {
            plan,
            condition_queries: Vec::new(),
            expected_graph_revision: 0,
        })
        .unwrap();
    }
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

fn character_impulse(target_id: &str, goal: &str) -> crate::domain::narrative_graph::effect::CharacterImpulse {
    crate::domain::narrative_graph::effect::CharacterImpulse {
        source_node: crate::domain::asset::ids::NarrativeNodeKey::try_new("node.impulse").unwrap(),
        target_role_id: RoleId::try_new(target_id).unwrap(),
        goal: bounded(goal),
        reason: None,
        emotion: None,
        urgency: crate::domain::narrative_graph::effect::ImpulseUrgency::Medium,
        expires_after_turn: None,
    }
}

#[test]
fn character_impulse_is_target_scoped() {
    let player = story_role("protagonist", RoleController::Player(PlayerId::try_new("player-1").unwrap()));
    let npc_a = story_role("npc-a", RoleController::Ai);
    let npc_b = story_role("npc-b", RoleController::Ai);
    let all_roles = [&player, &npc_a, &npc_b];
    let baseline = sample_baseline(&player, &[]);
    let ctx = build_context_with_impulses(
        &all_roles,
        baseline,
        vec![
            character_impulse("npc-a", "npc-a private goal"),
            character_impulse("npc-b", "npc-b private goal"),
        ],
    );
    let projector = DefaultCharacterThinkPromptContextProjector::new(
        crate::config::CharacterThinkConfig::default(),
        ContextPreparationConfig::default(),
    );
    let request = CharacterThinkRequest {
        role_id: npc_a.role_id.clone(),
        reason: bounded("assess the intruder"),
    };

    let projection = projector.project(&ctx, &request).expect("target role must be projectable");

    assert_eq!(projection.context.narrative_character_impulses.len(), 1);
    assert_eq!(
        projection.context.narrative_character_impulses[0].goal.as_str(),
        "npc-a private goal"
    );

    let rendered = projection
        .rc_vars
        .as_map()
        .get("narrative_character_impulses")
        .and_then(Value::as_str)
        .expect("narrative_character_impulses rendered");
    assert!(rendered.contains("npc-a private goal"));
    assert!(!rendered.contains("npc-b private goal"));
}

#[test]
fn runtime_context_projectors_preserve_slot_key_sets() {
    let player = story_role("protagonist", RoleController::Player(PlayerId::try_new("player-1").unwrap()));
    let npc_a = story_role("npc-a", RoleController::Ai);
    let all_roles = [&player, &npc_a];
    let baseline = sample_baseline(&player, &[]);
    let ctx = build_context(&all_roles, baseline);
    let projector = DefaultCharacterThinkPromptContextProjector::new(
        crate::config::CharacterThinkConfig::default(),
        ContextPreparationConfig::default(),
    );
    let request = CharacterThinkRequest {
        role_id: npc_a.role_id.clone(),
        reason: bounded("assess the intruder"),
    };

    let projection = projector.project(&ctx, &request).expect("target role must be projectable");

    let expected: std::collections::BTreeSet<&str> = [
        "target_character",
        "current_character_state",
        "story_summary",
        "recent_story",
        "narrative_character_impulses",
        "thinking_focus",
        "player_contribution",
    ]
    .into_iter()
    .collect();
    let actual: std::collections::BTreeSet<&str> = projection.rc_vars.as_map().keys().map(String::as_str).collect();
    assert_eq!(
        actual, expected,
        "character_think RC slot keys must match assets/prompts/context-v2/slots.yaml exactly"
    );
}

fn retrieved_knowledge_item(
    source_id: crate::domain::knowledge::KnowledgeSourceId,
    body: &str,
) -> crate::domain::turn::RetrievedKnowledgeItem {
    crate::domain::turn::RetrievedKnowledgeItem::from_parts(
        source_id,
        bounded(body),
        crate::domain::knowledge::KnowledgeSource::Seed {
            pack_id: PackId::try_new("pack-1").unwrap(),
            pack_digest: digest(),
        },
        crate::domain::turn::RelevanceRank {
            match_level: crate::domain::turn::MatchLevel::Entity,
            signal_priority: 0,
            salience: 1,
        },
        BTreeMap::new(),
    )
}

fn retrieval_limits() -> crate::domain::turn::RetrievedContextLimits {
    crate::domain::turn::RetrievedContextLimits {
        max_role_audiences: 8,
        max_items_per_audience: 8,
        max_tokens_per_audience: 10_000,
        max_total_items: 32,
        max_total_tokens: 10_000,
        max_item_bytes: 4096,
    }
}

fn build_context_with_retrieval(
    all_roles: &[&StoryRole],
    baseline: BaselineContext,
    think_targets: Vec<&str>,
    retrieved: crate::domain::turn::RetrievedContext,
) -> TurnExecutionContext {
    let budget = TurnBudget::from_config(
        &TurnConfig::default(),
        &TurnContentLimitsConfig::default(),
        &RetrievalConfig::default(),
        &StateExtractorConfig::default(),
        &NarrativeConfig::default(),
    )
    .unwrap();
    let identity = TurnIdentity::new(
        TurnKey::new(StoryId::try_new("story-1").unwrap(), TurnNumber::try_new(1).unwrap()),
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
        character_think_requests: think_targets
            .into_iter()
            .map(|id| CharacterThinkRequest {
                role_id: RoleId::try_new(id).unwrap(),
                reason: bounded("think"),
            })
            .collect(),
    };
    ctx.set_writer_plan(plan).unwrap();
    ctx.set_retrieved_context(retrieved).unwrap();
    ctx
}

#[test]
fn character_think_nests_authorized_knowledge_under_target_role() {
    let player = story_role("protagonist", RoleController::Player(PlayerId::try_new("player-1").unwrap()));
    let npc = story_role("npc-a", RoleController::Ai);
    let all_roles = [&player, &npc];
    let baseline = sample_baseline(&player, &[]);
    let mut characters = BTreeMap::new();
    characters.insert(
        npc.role_id.clone(),
        crate::domain::turn::RetrievedCharacterContext {
            role: None,
            known_rumors: vec![retrieved_knowledge_item(
                crate::domain::knowledge::KnowledgeSourceId::Rumor(
                    crate::domain::ids::RumorId::try_new("rumor_0001").unwrap(),
                ),
                "npc-a heard a rumor",
            )],
            memories: vec![retrieved_knowledge_item(
                crate::domain::knowledge::KnowledgeSourceId::Memory(
                    crate::domain::ids::MemoryId::try_new("memory_0001").unwrap(),
                ),
                "npc-a remembers an old promise",
            )],
        },
    );
    let retrieved = crate::domain::turn::RetrievedContext::try_new(
        crate::domain::turn::RetrievedWorldKnowledge::default(),
        characters,
        retrieval_limits(),
    )
    .unwrap();
    let ctx = build_context_with_retrieval(&all_roles, baseline, vec!["npc-a"], retrieved);
    let projector = DefaultCharacterThinkPromptContextProjector::new(
        crate::config::CharacterThinkConfig::default(),
        ContextPreparationConfig::default(),
    );
    let request = CharacterThinkRequest {
        role_id: npc.role_id.clone(),
        reason: bounded("assess the intruder"),
    };

    let projection = projector.project(&ctx, &request).expect("target role must be projectable");

    assert_eq!(projection.context.target_role.knowledge.known_rumors.len(), 1);
    assert_eq!(
        projection.context.target_role.knowledge.known_rumors[0].as_str(),
        "npc-a heard a rumor"
    );
    assert_eq!(projection.context.target_role.knowledge.memories.len(), 1);
    assert_eq!(
        projection.context.target_role.knowledge.memories[0].as_str(),
        "npc-a remembers an old promise"
    );

    let values = projection.rc_vars.as_map();
    assert!(!values.contains_key("knowledge"));
    assert!(!values.contains_key("role_knowledge"));
    let rendered = values
        .get("target_character")
        .and_then(Value::as_str)
        .expect("target_character rendered");
    assert!(rendered.starts_with("role_id: \"npc-a\""));
    assert!(rendered.contains("npc-a heard a rumor"));
    assert!(rendered.contains("npc-a remembers an old promise"));
}
