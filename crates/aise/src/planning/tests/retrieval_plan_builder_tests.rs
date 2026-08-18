use super::*;
use crate::domain::asset::character_card::CharacterProfile;
use crate::domain::asset::frozen_ref::FrozenStoryPackRef;
use crate::domain::asset::ids::{
    LocationKey, NarrativeNodeKey, PackId, PlayerId, SemanticVersion, Sha256Digest, StoryPackKey,
};
use crate::domain::asset::story_pack::{StoryProfile, StoryStyle};
use crate::domain::ids::{StoryId, StoryRevision};
use crate::domain::narrative::{StoryContinuity, StoryContinuityLimits, StorySummary};
use crate::domain::narrative_graph::definition::NarrativeGraphDefinition;
use crate::domain::narrative_graph::effect::ImpulseUrgency;
use crate::domain::narrative_graph::state::NarrativeRuntimeState;
use crate::domain::story_instance::role::{RoleController, StoryRoleState, StoryRoleView};
use crate::domain::story_instance::snapshot::{KnowledgeSnapshotRef, StoryReadSnapshotParts};
use crate::domain::story_instance::state::InstanceSettings;
use crate::domain::turn::{
    NarrativeGraphStateIndex, RelevantWorldKnowledge, RetrievalSignals, RoleContextView, RoleIndexEntry,
};
use crate::planning::planner_output::{
    CharacterThinkRequestDto, PlannerCharacterContextGapDto, PlannerWriterContextGapDto, WriterPlannerOutputDto,
};
use crate::planning::writer_planner_prompt::WriterPlannerPromptContextProjector;

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
        story_title: text("Untitled Story"),
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
        role_index: indexed_ai_ids
            .iter()
            .map(|id| RoleIndexEntry {
                role_id: RoleId::try_new(*id).unwrap(),
                retrieval_hint: text("hint"),
            })
            .collect(),
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

fn story_role_view(id: &str, controller: RoleController) -> StoryRoleView {
    StoryRoleView {
        role_id: RoleId::try_new(id).unwrap(),
        controller,
        role_label: text(id),
        narrative_function: text("narrative-function"),
        background: None,
        effective_profile: CharacterProfile {
            name: text(id),
            appearance: None,
            personality: None,
            speaking_style: None,
            dialogue_examples: Vec::new(),
        },
        source_character_id: None,
        state: StoryRoleState {
            location: LocationKey::from("hall"),
            goals: Vec::new(),
            attributes: BTreeMap::new(),
        },
    }
}

fn sample_snapshot(player_id: &str, ai_ids: &[&str]) -> StoryReadSnapshot {
    let mut roles = BTreeMap::new();
    roles.insert(
        RoleId::try_new(player_id).unwrap(),
        story_role_view(player_id, RoleController::Player(PlayerId::try_new("player-1").unwrap())),
    );
    for id in ai_ids {
        roles.insert(RoleId::try_new(*id).unwrap(), story_role_view(id, RoleController::Ai));
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
        story_title: text("Untitled Story"),
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
        roles,
        relationships: Vec::new(),
        narrative_definition: NarrativeGraphDefinition {
            entry_nodes: Vec::new(),
            nodes: BTreeMap::new(),
            edges: Vec::new(),
        },
        narrative_state: NarrativeRuntimeState::initial(),
        fact_values: BTreeMap::new(),
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
        active_constraints: Vec::new(),
        entity_catalog: Vec::new(),
        topic_dictionary: BTreeMap::new(),
        knowledge_snapshot: KnowledgeSnapshotRef {
            story_id: StoryId::try_new("story-1").unwrap(),
            pack_digest: digest(),
            base_revision: StoryRevision::new(0),
            knowledge_id_high_water: crate::domain::knowledge::KnowledgeIdHighWater::zero(),
        },
    })
    .unwrap()
}

fn writer_prompt_context(baseline: &BaselineContext, plan: &NarrativePlan) -> WriterPlannerPromptContext {
    WriterPlannerPromptContextProjector
        .project(baseline, plan, &text("go north"), &PlannerConfig::default(), 1_000_000)
        .expect("writer planner projection")
        .context
}

fn writer_gap(target_id: &str, reason: &str) -> PlannerWriterContextGapDto {
    PlannerWriterContextGapDto {
        target_id: target_id.to_owned(),
        reason: reason.to_owned(),
    }
}

fn character_gap(role_id: &str, target_id: &str, reason: &str) -> PlannerCharacterContextGapDto {
    PlannerCharacterContextGapDto {
        role_id: role_id.to_owned(),
        target_id: target_id.to_owned(),
        reason: reason.to_owned(),
    }
}

fn writer_planner_output(
    story_goal: &str,
    writer_context_gaps: Vec<PlannerWriterContextGapDto>,
    character_context_gaps: Vec<PlannerCharacterContextGapDto>,
    character_think_requests: Vec<CharacterThinkRequest>,
) -> WriterPlannerOutputDto {
    WriterPlannerOutputDto {
        story_goal: story_goal.to_owned(),
        writer_context_gaps,
        character_context_gaps,
        character_think_requests: character_think_requests
            .into_iter()
            .map(|request| CharacterThinkRequestDto {
                role_id: request.role_id.as_str().to_owned(),
                reason: request.reason.as_str().to_owned(),
            })
            .collect(),
    }
}

fn builder() -> RetrievalPlanBuilder {
    RetrievalPlanBuilder::new(RetrievalConfig::default(), PlannerConfig::default())
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
fn narrative_impulse_merges_character_think_request_once() {
    let baseline = baseline_with_roles("protagonist", &["npc-a", "npc-b"]);
    let planner_requests = vec![think_request("npc-a", "planner reason")];
    let impulses = vec![
        impulse("npc-b", "explore alone", Some("impulse-only reason")),
        impulse("npc-a", "first goal", Some("first reason")),
        impulse("npc-a", "second goal", Some("second reason")),
    ];
    let config = PlannerConfig::default();

    let merged = merge_narrative_think_requests(planner_requests, &impulses, &baseline, &config).unwrap();

    assert_eq!(merged.len(), 2);
    let by_role: BTreeMap<_, _> = merged.iter().map(|request| (request.role_id.as_str(), request)).collect();
    assert_eq!(by_role["npc-a"].reason.as_str(), "planner reason");
    assert_eq!(by_role["npc-b"].reason.as_str(), "impulse-only reason");

    let impulses_for_npc_a = impulses
        .iter()
        .filter(|impulse| impulse.target_role_id.as_str() == "npc-a")
        .count();
    assert_eq!(
        impulses_for_npc_a, 2,
        "CharacterThink must still see every impulse for the collapsed role"
    );
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

#[test]
fn indexed_character_target_loads_role_context_bundle() {
    let baseline = baseline_with_roles("protagonist", &["npc-a"]);
    let snapshot = sample_snapshot("protagonist", &["npc-a"]);
    let plan = NarrativePlan::empty();
    let writer_context = writer_prompt_context(&baseline, &plan);
    let output = writer_planner_output("goal", vec![writer_gap("npc-a", "recall the guard")], Vec::new(), Vec::new());

    let writer_plan = builder().build(&baseline, &plan, output, &snapshot, &writer_context).unwrap();

    assert_eq!(writer_plan.retrieval_plan.character_requests.len(), 1);
    let request = &writer_plan.retrieval_plan.character_requests[0];
    assert_eq!(request.role_id.as_str(), "npc-a");
    assert_eq!(request.origin, RetrievalRequestOrigin::Planner);
    assert_eq!(request.reason.as_str(), "recall the guard");
    assert!(writer_plan.retrieval_plan.knowledge_requests.iter().any(|request| {
        matches!(&request.delivery, KnowledgeDelivery::Character { role_id } if role_id.as_str() == "npc-a")
            && request.entities == vec![KnowledgeEntity::Role(RoleId::try_new("npc-a").unwrap())]
    }));
}

#[test]
fn character_think_automatically_retrieves_role_cognition() {
    let baseline = baseline_with_roles("protagonist", &["npc-a"]);
    let snapshot = sample_snapshot("protagonist", &["npc-a"]);
    let plan = NarrativePlan::empty();
    let writer_context = writer_prompt_context(&baseline, &plan);
    let output = writer_planner_output(
        "goal",
        Vec::new(),
        Vec::new(),
        vec![think_request("npc-a", "assess the visitor")],
    );

    let writer_plan = builder().build(&baseline, &plan, output, &snapshot, &writer_context).unwrap();

    assert_eq!(writer_plan.retrieval_plan.character_requests.len(), 1);
    let request = &writer_plan.retrieval_plan.character_requests[0];
    assert_eq!(request.role_id.as_str(), "npc-a");
    assert_eq!(request.origin, RetrievalRequestOrigin::Automatic);
    let knowledge_request = writer_plan
        .retrieval_plan
        .knowledge_requests
        .iter()
        .find(|request| matches!(&request.delivery, KnowledgeDelivery::Character { role_id } if role_id.as_str() == "npc-a"))
        .expect("automatic role cognition knowledge request");
    assert_eq!(
        knowledge_request.knowledge_kinds,
        vec![KnowledgeKind::Rumor, KnowledgeKind::Memory]
    );
    assert_eq!(
        knowledge_request.entities,
        vec![KnowledgeEntity::Role(RoleId::try_new("npc-a").unwrap())]
    );
}

#[test]
fn role_cognition_request_deduplicates_by_role() {
    let baseline = baseline_with_roles("protagonist", &["npc-a"]);
    let snapshot = sample_snapshot("protagonist", &["npc-a"]);
    let plan = NarrativePlan::empty();
    let writer_context = writer_prompt_context(&baseline, &plan);
    let output = writer_planner_output(
        "goal",
        vec![writer_gap("npc-a", "planner target reason")],
        Vec::new(),
        vec![think_request("npc-a", "think reason")],
    );

    let writer_plan = builder().build(&baseline, &plan, output, &snapshot, &writer_context).unwrap();

    assert_eq!(writer_plan.retrieval_plan.character_requests.len(), 1);
    let request = &writer_plan.retrieval_plan.character_requests[0];
    assert_eq!(request.origin, RetrievalRequestOrigin::Planner);
    assert_eq!(request.reason.as_str(), "planner target reason");
    let cognition_knowledge_requests = writer_plan
        .retrieval_plan
        .knowledge_requests
        .iter()
        .filter(|request| matches!(&request.delivery, KnowledgeDelivery::Character { role_id } if role_id.as_str() == "npc-a"))
        .count();
    assert_eq!(cognition_knowledge_requests, 1);
}

#[test]
fn indexed_target_audience_matrix_is_enforced() {
    let baseline = baseline_with_roles("protagonist", &["npc-a"]);
    let snapshot = sample_snapshot("protagonist", &["npc-a"]);
    let plan = NarrativePlan::empty();
    let writer_context = writer_prompt_context(&baseline, &plan);

    let role_to_character = writer_planner_output(
        "goal",
        Vec::new(),
        vec![character_gap("npc-a", "npc-a", "role target for character audience")],
        vec![think_request("npc-a", "think reason")],
    );
    let error = builder()
        .build(&baseline, &plan, role_to_character, &snapshot, &writer_context)
        .unwrap_err();
    assert!(matches!(error, PlanningError::KnowledgeAudienceViolation));

    let mut baseline_with_fact = baseline.clone();
    baseline_with_fact.knowledge_index = vec![crate::domain::turn::KnowledgeIndexEntry {
        source_id: crate::domain::knowledge::KnowledgeSourceId::Fact(
            crate::domain::ids::FactId::try_new("fact_0001").unwrap(),
        ),
        retrieval_hint: crate::domain::knowledge::RetrievalHint::try_new("hint".to_owned()).unwrap(),
    }];
    let writer_context_with_fact = writer_prompt_context(&baseline_with_fact, &plan);
    let fact_to_character = writer_planner_output(
        "goal",
        Vec::new(),
        vec![character_gap(
            "npc-a",
            "fact_0001",
            "fact target for character audience",
        )],
        vec![think_request("npc-a", "think reason")],
    );
    let error = builder()
        .build(
            &baseline_with_fact,
            &plan,
            fact_to_character,
            &snapshot,
            &writer_context_with_fact,
        )
        .unwrap_err();
    assert!(matches!(error, PlanningError::KnowledgeAudienceViolation));

    let mut baseline_with_rumor = baseline.clone();
    baseline_with_rumor.knowledge_index = vec![crate::domain::turn::KnowledgeIndexEntry {
        source_id: crate::domain::knowledge::KnowledgeSourceId::Rumor(
            crate::domain::ids::RumorId::try_new("rumor_0001").unwrap(),
        ),
        retrieval_hint: crate::domain::knowledge::RetrievalHint::try_new("hint".to_owned()).unwrap(),
    }];
    let writer_context_with_rumor = writer_prompt_context(&baseline_with_rumor, &plan);
    let rumor_without_think = writer_planner_output(
        "goal",
        Vec::new(),
        vec![character_gap(
            "npc-a",
            "rumor_0001",
            "rumor target without a think request",
        )],
        Vec::new(),
    );
    let error = builder()
        .build(
            &baseline_with_rumor,
            &plan,
            rumor_without_think,
            &snapshot,
            &writer_context_with_rumor,
        )
        .unwrap_err();
    assert!(matches!(error, PlanningError::KnowledgeAudienceViolation));
}
