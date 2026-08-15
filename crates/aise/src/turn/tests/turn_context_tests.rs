use super::*;
use crate::config::{NarrativeConfig, RetrievalConfig, StateExtractorConfig, TurnConfig, TurnContentLimitsConfig};
use crate::domain::asset::character_card::{
    AssetSpecVersion, CharacterCard, CharacterMeta, CharacterProfile, CharacterSpec, SpeakingStyle,
};
use crate::domain::asset::frozen_ref::FrozenCharacterAssetRef;
use crate::domain::asset::ids::{
    CharacterAssetKey, LocationKey, PackId, PlayerId, SceneKey, SemanticVersion, Sha256Digest, StoryPackKey,
    StoryRoleKey,
};
use crate::domain::asset::story_pack::{InitialRoleState, StoryProfile, StoryRole, StoryStyle};
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::StoryRevision;
use crate::domain::narrative::{StoryContinuity, StoryContinuityLimits, StorySummary};
use crate::domain::narrative_graph::definition::NarrativeGraphDefinition;
use crate::domain::narrative_graph::state::NarrativeRuntimeState;
use crate::domain::story_instance::binding::{RoleBinding, RoleController};
use crate::domain::story_instance::snapshot::{KnowledgeSnapshotRef, StoryReadSnapshotParts};
use crate::domain::story_instance::state::{CharacterInstanceState, CurrentScene, InstanceSettings};
use crate::domain::turn::{
    CharacterThinkRequest, CharacterView, NarrativeGraphStateIndex, RetrievalIndexScope, RetrievalPlan,
    RetrievalSignals, WriterStoryGoal,
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
        premise: bounded("premise"),
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

fn scene() -> CurrentScene {
    CurrentScene {
        scene_key: SceneKey::from("scene_1"),
        location_key: LocationKey::from("village"),
        time: bounded("morning"),
        description: bounded("scene"),
        present_character_ids: Vec::new(),
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

fn player_view() -> CharacterView {
    let character_id = CharacterId::from("player-1");
    let role_key = StoryRoleKey::from("protagonist");
    let card = CharacterCard {
        spec: CharacterSpec::V3,
        spec_version: AssetSpecVersion::V3_0,
        character_key: CharacterAssetKey::from("player"),
        meta: CharacterMeta {
            name: bounded("Player"),
            creator: None,
            version: SemanticVersion::try_new("0.1.0").unwrap(),
            tags: Vec::new(),
        },
        profile: CharacterProfile {
            description: bounded("player"),
            personality: Vec::new(),
            values: Vec::new(),
            fears: Vec::new(),
            speaking_style: SpeakingStyle {
                register: bounded("neutral"),
                verbosity: bounded("medium"),
                traits: Vec::new(),
            },
            dialogue_examples: Vec::new(),
        },
    };
    let role = StoryRole {
        role_label: bounded("Protagonist"),
        narrative_function: bounded("hero"),
        initial_state: InitialRoleState {
            location: LocationKey::from("village"),
            goals: Vec::new(),
            attributes: BTreeMap::new(),
        },
        initial_relationships: Vec::new(),
        seed_memories: Vec::new(),
    };
    let binding = RoleBinding {
        role_key: role_key.clone(),
        character_id: character_id.clone(),
        character_asset: FrozenCharacterAssetRef {
            character_key: card.character_key.clone(),
            version: card.meta.version.clone(),
            digest: digest(),
        },
        controller: RoleController::Player(PlayerId::from("player-account-1")),
        bound_at_ms: 0,
    };
    let state = CharacterInstanceState {
        character_id: character_id.clone(),
        role_key: role_key.clone(),
        location: LocationKey::from("village"),
        goals: Vec::new(),
        attributes: BTreeMap::new(),
    };
    CharacterView {
        character_id,
        role_key,
        role,
        binding,
        card,
        state,
    }
}

fn sample_baseline(player: &CharacterView) -> BaselineContext {
    BaselineContext {
        story_profile: story_profile(),
        instance_settings: InstanceSettings::default(),
        player_character: player.clone(),
        current_scene: scene(),
        scene_characters: Vec::new(),
        referenced_characters: Vec::new(),
        relevant_knowledge: Vec::new(),
        character_index_scope: RetrievalIndexScope::Complete,
        knowledge_entry_index_scope: RetrievalIndexScope::Complete,
        knowledge_entry_index: Vec::new(),
        character_index: Vec::new(),
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

fn sample_snapshot(player: &CharacterView) -> StoryReadSnapshot {
    let mut role_definitions = BTreeMap::new();
    role_definitions.insert(player.role_key.clone(), player.role.clone());
    let mut role_bindings = BTreeMap::new();
    role_bindings.insert(player.role_key.clone(), player.binding.clone());
    let mut character_cards = BTreeMap::new();
    character_cards.insert(player.character_id.clone(), player.card.clone());
    let mut character_states = BTreeMap::new();
    character_states.insert(player.character_id.clone(), player.state.clone());
    StoryReadSnapshot::try_from_parts(StoryReadSnapshotParts {
        story_id: StoryId::try_new("story-1").unwrap(),
        base_revision: StoryRevision::new(0),
        pack: crate::domain::asset::frozen_ref::FrozenStoryPackRef {
            pack_id: PackId::from("pack-1"),
            pack_key: StoryPackKey::from("pack-1"),
            version: SemanticVersion::try_new("0.1.0").unwrap(),
            digest: digest(),
        },
        story_profile: story_profile(),
        instance_settings: InstanceSettings::default(),
        role_definitions,
        role_bindings,
        character_cards,
        character_states,
        current_scene: scene(),
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

fn think_request(id: &str) -> CharacterThinkRequest {
    CharacterThinkRequest {
        character_id: CharacterId::from(id),
        reason: bounded("present"),
    }
}

fn decision(id: &str, text: &str) -> CharacterDecision {
    CharacterDecision {
        character_id: CharacterId::from(id),
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
        StoryId::try_new("story-1").unwrap(),
        TurnId::try_new("turn-1").unwrap(),
        IdempotencyKey::try_new("idem-1").unwrap(),
        0,
    );
    let request = TurnRequest::try_new("go north".to_owned()).unwrap();
    let control = TurnControl::new(std::time::Instant::now() + Duration::from_secs(30), TurnCancellation::new());
    let trace = TraceRecorder::with_limits(budget.max_trace_spans());
    let mut ctx = TurnExecutionContext::new(identity, request, budget, control, trace).unwrap();
    ctx.complete_initialization().unwrap();
    let player = player_view();
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
        vec![think_request("player-1")],
    );
    let err = ctx.set_character_decisions(vec![decision("player-1", "act")]).unwrap_err();
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
    let before: Vec<CharacterId> = ctx.character_decisions().iter().map(|item| item.character_id.clone()).collect();
    let err = ctx.set_character_decisions(Vec::new()).unwrap_err();
    assert_eq!(err.code(), "character_decision_count_mismatch");
    let after: Vec<CharacterId> = ctx.character_decisions().iter().map(|item| item.character_id.clone()).collect();
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
fn old_turn_config_key_is_not_accepted_as_alias() {
    let mut value = serde_json::to_value(TurnConfig::default()).unwrap();
    let object = value.as_object_mut().unwrap();
    object.remove("max_character_decisions");
    object.insert("max_character_thoughts".to_owned(), serde_json::Value::from(99));
    let config: TurnConfig = serde_json::from_value(value).unwrap();
    assert_eq!(config.max_character_decisions, TurnConfig::default().max_character_decisions);
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
