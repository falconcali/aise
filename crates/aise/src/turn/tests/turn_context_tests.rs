use super::*;
use crate::config::{NarrativeConfig, RetrievalConfig, StateExtractorConfig, TurnConfig, TurnContentLimitsConfig};
use crate::domain::asset::character_card::CharacterProfile;
use crate::domain::asset::ids::{LocationKey, PackId, PlayerId, SemanticVersion, Sha256Digest, StoryPackKey};
use crate::domain::asset::story_pack::{StoryProfile, StoryStyle};
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::{RoleId, StoryRevision, TurnKey, TurnNumber};
use crate::domain::narrative::{StoryContinuity, StoryContinuityLimits, StorySummary};
use crate::domain::narrative_graph::definition::NarrativeGraphDefinition;
use crate::domain::narrative_graph::state::NarrativeRuntimeState;
use crate::domain::story_instance::role::{RoleController, StoryRole, StoryRoleState};
use crate::domain::story_instance::snapshot::{KnowledgeSnapshotRef, StoryReadSnapshotParts};
use crate::domain::story_instance::state::InstanceSettings;
use crate::domain::turn::{
    CharacterThinkRequest, NarrativeGraphStateIndex, RetrievalPlan, RetrievalSignals, RoleContextView, WriterStoryGoal,
};
use crate::turn::turn_contract::{IdempotencyKey, TurnCancellation};
use std::collections::BTreeMap;
use std::time::Duration;

fn bounded(text: &str) -> BoundedText {
    BoundedText::try_new(text, "text", 4096).unwrap()
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

fn player_role() -> StoryRole {
    StoryRole {
        role_id: RoleId::try_new("protagonist").unwrap(),
        controller: RoleController::Player(PlayerId::try_new("player-account-1").unwrap()),
        role_label: bounded("Protagonist"),
        narrative_function: bounded("hero"),
        background: None,
        effective_profile: CharacterProfile {
            name: bounded("Player"),
            appearance: None,
            personality: None,
            speaking_style: None,
            dialogue_examples: Vec::new(),
        },
        source_character: None,
        state: StoryRoleState {
            location: LocationKey::from("village"),
            goals: Vec::new(),
            attributes: BTreeMap::new(),
        },
    }
}

fn sample_baseline(player: &StoryRole) -> BaselineContext {
    BaselineContext {
        story_title: bounded("Untitled Story"),
        story_profile: story_profile(),
        instance_settings: InstanceSettings::default(),
        player_role: RoleContextView::from(&crate::domain::story_instance::role::StoryRoleView::from(player)),
        relevant_roles: Vec::new(),
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

fn sample_snapshot(player: &StoryRole) -> StoryReadSnapshot {
    let mut roles = BTreeMap::new();
    roles.insert(
        player.role_id.clone(),
        crate::domain::story_instance::role::StoryRoleView::from(player),
    );
    StoryReadSnapshot::try_from_parts(StoryReadSnapshotParts {
        story_id: StoryId::try_new("story-1").unwrap(),
        base_revision: StoryRevision::new(0),
        pack: crate::domain::asset::frozen_ref::FrozenStoryPackRef {
            pack_id: PackId::from("pack-1"),
            pack_key: StoryPackKey::from("pack-1"),
            version: SemanticVersion::try_new("0.1.0").unwrap(),
            digest: digest(),
        },
        story_title: bounded("Untitled Story"),
        story_profile: story_profile(),
        instance_settings: InstanceSettings::default(),
        roles,
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

fn think_request(id: &str) -> CharacterThinkRequest {
    CharacterThinkRequest {
        role_id: RoleId::try_new(id).unwrap(),
        reason: bounded("present"),
    }
}

fn decision(id: &str, text: &str) -> CharacterDecision {
    CharacterDecision {
        role_id: RoleId::try_new(id).unwrap(),
        decision: bounded(text),
        suggested_utterance: None,
    }
}

fn build_ready_context(
    turn_config: TurnConfig,
    content_config: TurnContentLimitsConfig,
    requests: Vec<CharacterThinkRequest>,
) -> TurnExecutionContext {
    let budget = TurnBudget::from_config(
        &turn_config,
        &content_config,
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
    let control = TurnControl::new(std::time::Instant::now() + Duration::from_secs(30), TurnCancellation::new());
    let trace = TraceRecorder::with_limits(budget.max_trace_spans());
    let mut ctx = TurnExecutionContext::new(identity, request, budget, control, trace).unwrap();
    ctx.complete_initialization().unwrap();
    let player = player_role();
    ctx.set_prepared_context(sample_snapshot(&player), sample_baseline(&player))
        .unwrap();
    let plan = WriterPlan {
        story_goal: WriterStoryGoal {
            summary: bounded("goal"),
        },
        retrieval_plan: RetrievalPlan::default(),
        character_think_requests: requests,
    };
    ctx.set_writer_plan(plan).unwrap();
    ctx
}

#[test]
fn exact_request_and_decision_count_succeeds() {
    let mut ctx = build_ready_context(
        TurnConfig::default(),
        TurnContentLimitsConfig::default(),
        vec![think_request("npc-1")],
    );
    assert!(ctx.set_character_decisions(vec![decision("npc-1", "act now")]).is_ok());
    assert_eq!(ctx.character_decisions().len(), 1);
}

#[test]
fn count_mismatch_fails_with_expected_code() {
    let mut ctx = build_ready_context(
        TurnConfig::default(),
        TurnContentLimitsConfig::default(),
        vec![think_request("npc-1")],
    );
    let err = ctx.set_character_decisions(Vec::new()).unwrap_err();
    assert_eq!(err.code(), "character_decision_count_mismatch");
}

#[test]
fn order_mismatch_fails_with_expected_code() {
    let mut ctx = build_ready_context(
        TurnConfig::default(),
        TurnContentLimitsConfig::default(),
        vec![think_request("npc-1"), think_request("npc-2")],
    );
    let err = ctx
        .set_character_decisions(vec![decision("npc-2", "act"), decision("npc-1", "act")])
        .unwrap_err();
    assert_eq!(err.code(), "character_decision_order_mismatch");
}

#[test]
fn duplicate_target_fails_with_expected_code() {
    let mut ctx = build_ready_context(
        TurnConfig::default(),
        TurnContentLimitsConfig::default(),
        vec![think_request("npc-1"), think_request("npc-1")],
    );
    let err = ctx
        .set_character_decisions(vec![decision("npc-1", "act"), decision("npc-1", "act")])
        .unwrap_err();
    assert_eq!(err.code(), "duplicate_character_decision");
}

#[test]
fn player_character_target_fails_before_assignment() {
    let mut ctx = build_ready_context(
        TurnConfig::default(),
        TurnContentLimitsConfig::default(),
        vec![think_request("protagonist")],
    );
    let err = ctx.set_character_decisions(vec![decision("protagonist", "act")]).unwrap_err();
    assert_eq!(err.code(), "character_think_player_target");
    assert!(ctx.character_decisions().is_empty());
}

#[test]
fn count_limit_fails_with_expected_code() {
    let turn_config = TurnConfig {
        max_character_decisions: 1,
        ..TurnConfig::default()
    };
    let mut ctx = build_ready_context(
        turn_config,
        TurnContentLimitsConfig::default(),
        vec![think_request("npc-1"), think_request("npc-2")],
    );
    let err = ctx
        .set_character_decisions(vec![decision("npc-1", "act"), decision("npc-2", "act")])
        .unwrap_err();
    assert_eq!(err.code(), "character_decision_limit");
}

#[test]
fn aggregate_byte_limit_fails_with_expected_code() {
    let content_config = TurnContentLimitsConfig {
        max_character_decision_bytes: 8,
        ..TurnContentLimitsConfig::default()
    };
    let mut ctx = build_ready_context(TurnConfig::default(), content_config, vec![think_request("npc-1")]);
    let err = ctx
        .set_character_decisions(vec![decision("npc-1", "a decision far longer than eight bytes")])
        .unwrap_err();
    assert_eq!(err.code(), "character_decision_byte_limit");
}

#[test]
fn failed_assignment_leaves_previous_collection_unchanged() {
    let mut ctx = build_ready_context(
        TurnConfig::default(),
        TurnContentLimitsConfig::default(),
        vec![think_request("npc-1")],
    );
    ctx.set_character_decisions(vec![decision("npc-1", "act")]).unwrap();
    let before: Vec<RoleId> = ctx.character_decisions().iter().map(|item| item.role_id.clone()).collect();
    let err = ctx.set_character_decisions(Vec::new()).unwrap_err();
    assert_eq!(err.code(), "character_decision_count_mismatch");
    let after: Vec<RoleId> = ctx.character_decisions().iter().map(|item| item.role_id.clone()).collect();
    assert_eq!(before, after);
    assert_eq!(ctx.character_decisions().len(), 1);
}

#[test]
fn skip_is_valid_only_for_empty_request_list() {
    let mut empty_ctx = build_ready_context(TurnConfig::default(), TurnContentLimitsConfig::default(), Vec::new());
    assert!(empty_ctx.skip_character_thinking().is_ok());
    assert!(empty_ctx.character_decisions().is_empty());

    let mut non_empty_ctx = build_ready_context(
        TurnConfig::default(),
        TurnContentLimitsConfig::default(),
        vec![think_request("npc-1")],
    );
    let err = non_empty_ctx.skip_character_thinking().unwrap_err();
    assert_eq!(err.code(), "character_thinking_not_skippable");
}

#[test]
fn renamed_configuration_values_flow_into_turn_budget() {
    let default_budget = TurnBudget::from_config(
        &TurnConfig::default(),
        &TurnContentLimitsConfig::default(),
        &RetrievalConfig::default(),
        &StateExtractorConfig::default(),
        &NarrativeConfig::default(),
    )
    .unwrap();
    assert_eq!(default_budget.max_character_decisions(), 8);
    assert_eq!(default_budget.max_character_decision_bytes(), 1024);

    let explicit_budget = TurnBudget::from_config(
        &TurnConfig {
            max_character_decisions: 3,
            ..TurnConfig::default()
        },
        &TurnContentLimitsConfig {
            max_character_decision_bytes: 222,
            ..TurnContentLimitsConfig::default()
        },
        &RetrievalConfig::default(),
        &StateExtractorConfig::default(),
        &NarrativeConfig::default(),
    )
    .unwrap();
    assert_eq!(explicit_budget.max_character_decisions(), 3);
    assert_eq!(explicit_budget.max_character_decision_bytes(), 222);
}

#[test]
fn old_content_config_key_is_not_accepted_as_alias() {
    let mut value = serde_json::to_value(TurnContentLimitsConfig::default()).unwrap();
    let object = value.as_object_mut().unwrap();
    let bytes = object.remove("max_character_decision_bytes").unwrap();
    object.insert("max_character_thought_bytes".to_owned(), bytes);
    let result: Result<TurnContentLimitsConfig, _> = serde_json::from_value(value);
    assert!(result.is_err());
}
