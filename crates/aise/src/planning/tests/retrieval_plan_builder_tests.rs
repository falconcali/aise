use super::*;
use crate::domain::asset::character_card::CharacterProfile;
use crate::domain::asset::ids::{LocationKey, NarrativeNodeKey, PlayerId, Sha256Digest};
use crate::domain::asset::story_pack::{StoryProfile, StoryStyle};
use crate::domain::narrative::{StoryContinuity, StoryContinuityLimits, StorySummary};
use crate::domain::narrative_graph::effect::ImpulseUrgency;
use crate::domain::story_instance::role::{RoleController, StoryRoleState};
use crate::domain::story_instance::state::InstanceSettings;
use crate::domain::turn::{
    NarrativeGraphStateIndex, RelevantWorldKnowledge, RetrievalIndexScope, RetrievalSignalOrigin, RetrievalSignals,
    RoleContextView, RoleIndexEntry,
};

fn text(value: &str) -> BoundedText {
    BoundedText::try_new(value.to_owned(), "text", 256).unwrap()
}

fn digest() -> Sha256Digest {
    Sha256Digest::try_new("sha256:0000000000000000000000000000000000000000000000000000000000000000").unwrap()
}

fn role_view(id: &str, controller: RoleController) -> RoleContextView {
    RoleContextView {
        role_id: RoleId::try_new(id).unwrap(),
        role_label: text(id),
        narrative_function: text("narrative-function"),
        background: None,
        profile: CharacterProfile {
            name: text(id),
            appearance: None,
            personality: None,
            speaking_style: None,
            dialogue_examples: Vec::new(),
        },
        state: StoryRoleState {
            location: LocationKey::from("hall"),
            goals: Vec::new(),
            attributes: BTreeMap::new(),
        },
        controller,
    }
}

fn baseline_with_roles(player_id: &str, indexed_ai_ids: &[&str]) -> BaselineContext {
    BaselineContext {
        story_profile: StoryProfile {
            language: text("zh-CN"),
            genre: Vec::new(),
            themes: Vec::new(),
            style: StoryStyle {
                tone: Vec::new(),
                point_of_view: text("third"),
                tense: text("past"),
            },
        },
        instance_settings: InstanceSettings::default(),
        player_role: role_view(player_id, RoleController::Player(PlayerId::try_new("player-1").unwrap())),
        relevant_roles: Vec::new(),
        relevant_world_knowledge: RelevantWorldKnowledge::default(),
        role_index_scope: RetrievalIndexScope::Complete,
        role_index: indexed_ai_ids
            .iter()
            .map(|id| RoleIndexEntry {
                role_id: RoleId::try_new(*id).unwrap(),
                retrieval_hint: text("hint"),
            })
            .collect(),
        knowledge_index_scope: RetrievalIndexScope::Complete,
        knowledge_index: Vec::new(),
        story_continuity: StoryContinuity::try_new(
            StorySummary {
                text: text(""),
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

fn impulse(target_id: &str, goal: &str, reason: Option<&str>) -> CharacterImpulse {
    CharacterImpulse {
        source_node: NarrativeNodeKey::try_new("node.impulse").unwrap(),
        target_role_id: RoleId::try_new(target_id).unwrap(),
        goal: text(goal),
        reason: reason.map(text),
        emotion: None,
        urgency: ImpulseUrgency::Medium,
        expires_after_turn: None,
    }
}

fn think_request(id: &str, reason: &str) -> CharacterThinkRequest {
    CharacterThinkRequest {
        role_id: RoleId::try_new(id).unwrap(),
        reason: text(reason),
    }
}

#[test]
fn baseline_signal_entity_is_known_without_knowledge_catalog_entry() {
    let entity = KnowledgeEntity::Location(LocationKey::from("lodge_hall"));
    let signals = vec![EntitySignal {
        entity: entity.clone(),
        origin: RetrievalSignalOrigin::RoleState,
        priority: 1,
    }];

    assert!(entity_is_known(&entity, &[], &signals));
}

#[test]
fn arbitrary_location_is_not_known_without_catalog_or_signal() {
    let entity = KnowledgeEntity::Location(LocationKey::from("invented_location"));

    assert!(!entity_is_known(&entity, &[], &[]));
}

#[test]
fn merge_appends_impulse_targets_not_already_requested_sorted_by_role_id() {
    let baseline = baseline_with_roles("protagonist", &["npc-b", "npc-a"]);
    let planner_requests = vec![think_request("npc-b", "planner reason")];
    let impulses = vec![impulse("npc-a", "explore", Some("impulse reason"))];
    let config = PlannerConfig::default();

    let merged = merge_narrative_think_requests(planner_requests, &impulses, &baseline, &config).unwrap();

    assert_eq!(merged.len(), 2);
    assert_eq!(merged[0].role_id.as_str(), "npc-b");
    assert_eq!(merged[0].reason.as_str(), "planner reason");
    assert_eq!(merged[1].role_id.as_str(), "npc-a");
    assert_eq!(merged[1].reason.as_str(), "impulse reason");
}

#[test]
fn merge_uses_goal_when_impulse_reason_is_absent() {
    let baseline = baseline_with_roles("protagonist", &["npc-a"]);
    let impulses = vec![impulse("npc-a", "flee the scene", None)];
    let config = PlannerConfig::default();

    let merged = merge_narrative_think_requests(Vec::new(), &impulses, &baseline, &config).unwrap();

    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].reason.as_str(), "flee the scene");
}

#[test]
fn merge_collapses_multiple_impulses_for_one_role_into_one_request() {
    let baseline = baseline_with_roles("protagonist", &["npc-a"]);
    let impulses = vec![
        impulse("npc-a", "first goal", Some("first reason")),
        impulse("npc-a", "second goal", Some("second reason")),
    ];
    let config = PlannerConfig::default();

    let merged = merge_narrative_think_requests(Vec::new(), &impulses, &baseline, &config).unwrap();

    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].reason.as_str(), "first reason");
}

#[test]
fn merge_does_not_duplicate_a_role_already_requested_by_planner() {
    let baseline = baseline_with_roles("protagonist", &["npc-a"]);
    let planner_requests = vec![think_request("npc-a", "planner reason")];
    let impulses = vec![impulse("npc-a", "goal", Some("impulse reason"))];
    let config = PlannerConfig::default();

    let merged = merge_narrative_think_requests(planner_requests, &impulses, &baseline, &config).unwrap();

    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].reason.as_str(), "planner reason");
}

#[test]
fn merge_rejects_impulse_targeting_the_player_role() {
    let baseline = baseline_with_roles("protagonist", &[]);
    let impulses = vec![impulse("protagonist", "goal", Some("reason"))];
    let config = PlannerConfig::default();

    let error = merge_narrative_think_requests(Vec::new(), &impulses, &baseline, &config).unwrap_err();

    assert!(matches!(error, PlanningError::PlayerRoleRequested));
}

#[test]
fn merge_rejects_impulse_targeting_an_unknown_role() {
    let baseline = baseline_with_roles("protagonist", &[]);
    let impulses = vec![impulse("ghost-role", "goal", Some("reason"))];
    let config = PlannerConfig::default();

    let error = merge_narrative_think_requests(Vec::new(), &impulses, &baseline, &config).unwrap_err();

    assert!(matches!(error, PlanningError::UnknownRole));
}

#[test]
fn merge_rechecks_max_character_think_requests_after_merging_impulses() {
    let baseline = baseline_with_roles("protagonist", &["npc-a", "npc-b"]);
    let planner_requests = vec![think_request("npc-a", "planner reason")];
    let impulses = vec![impulse("npc-b", "goal", Some("reason"))];
    let config = PlannerConfig {
        max_character_think_requests: 1,
        ..PlannerConfig::default()
    };

    let error = merge_narrative_think_requests(planner_requests, &impulses, &baseline, &config).unwrap_err();

    assert!(matches!(
        error,
        PlanningError::LimitExceeded {
            limit: "max_character_think_requests"
        }
    ));
}
