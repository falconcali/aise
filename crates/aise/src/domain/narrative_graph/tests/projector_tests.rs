use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::ids::{
    AttributeKey, CanonicalEventKey, FactKey, NarrativeConditionKey, NarrativeNodeKey, StoryRoleKey,
};
use crate::domain::asset::validation::{BoundedText, ScalarValue};
use crate::domain::ids::CharacterId;
use crate::domain::narrative_graph::condition::{NarrativeCondition, RoleControllerKind, SemanticNarrativeCondition};
use crate::domain::narrative_graph::definition::{
    NarrativeGraphDefinition, NarrativeLimits, NarrativeNodeDefinition, NarrativeNodeEffects,
};
use crate::domain::narrative_graph::effect::{
    NarrativeEffectDefinition, NarrativeEffectId, NarrativeTransitionKind, WorldEventIntentDefinition,
};
use crate::domain::narrative_graph::projector::{
    NarrativeEffectDisposition, NarrativeProjectionInput, NarrativeProjector,
};
use crate::domain::narrative_graph::state::{NarrativeRuntimeState, PendingNarrativeEffect};
use crate::domain::narrative_graph::state_view::{NarrativeStateView, NarrativeStateViewError};
use std::collections::BTreeMap;

struct StubStateView;

impl NarrativeStateView for StubStateView {
    fn fact_value(&self, _fact_key: &FactKey) -> Result<Option<&ScalarValue>, NarrativeStateViewError> {
        Ok(None)
    }
    fn character_attribute(
        &self,
        _role_key: &StoryRoleKey,
        _attribute: &AttributeKey,
    ) -> Result<Option<&ScalarValue>, NarrativeStateViewError> {
        Ok(None)
    }
    fn relationship_trust(
        &self,
        _source_role_key: &StoryRoleKey,
        _target_role_key: &StoryRoleKey,
    ) -> Result<Option<i16>, NarrativeStateViewError> {
        Ok(None)
    }
    fn role_controller(&self, _role_key: &StoryRoleKey) -> Result<RoleControllerKind, NarrativeStateViewError> {
        Ok(RoleControllerKind::Ai)
    }
    fn character_id_for_role(&self, _role_key: &StoryRoleKey) -> Result<CharacterId, NarrativeStateViewError> {
        Ok(CharacterId::from("char.stub"))
    }
}

fn limits() -> NarrativeLimits {
    NarrativeLimits {
        max_graph_nodes: 64,
        max_graph_edges: 64,
        max_condition_depth: 8,
        max_conditions_per_node: 16,
        max_effects_per_node: 16,
        max_semantic_conditions: 16,
        max_semantic_criterion_bytes: 1024,
        max_frontier_nodes: 16,
        max_semantic_queries_per_turn: 16,
        max_semantic_query_bytes: 1024,
        max_evidence_bytes: 256,
        max_result_reason_bytes: 256,
        max_transitions_per_turn: 16,
        max_pending_effects: 16,
    }
}

fn simple_node(dramatic_focus: Option<&str>) -> NarrativeNodeDefinition {
    NarrativeNodeDefinition {
        title: BoundedText::try_new("Node", "title", 256).unwrap(),
        dramatic_focus: dramatic_focus.map(|text| BoundedText::try_new(text, "dramatic_focus", 256).unwrap()),
        activate_when: NarrativeCondition::StoryStarted,
        complete_when: NarrativeCondition::TurnReaches { turn: 999 },
        skip_when: None,
        effects: NarrativeNodeEffects {
            on_activate: Vec::new(),
            on_complete: Vec::new(),
        },
        terminal: false,
    }
}

#[test]
fn active_nodes_and_directions_are_collected_from_state() {
    let node_key = NarrativeNodeKey::try_new("node.intro").unwrap();
    let mut nodes = BTreeMap::new();
    nodes.insert(node_key.clone(), simple_node(Some("focus on the stranger")));
    let definition = NarrativeGraphDefinition {
        entry_nodes: vec![node_key.clone()],
        nodes,
        edges: Vec::new(),
    };
    let mut state = NarrativeRuntimeState::initial();
    state.node_states.insert(
        node_key.clone(),
        crate::domain::narrative_graph::condition::NarrativeNodeState::Active,
    );
    let view = StubStateView;
    let projector = NarrativeProjector::new(limits());
    let projection = projector
        .project(NarrativeProjectionInput {
            definition: &definition,
            state: &state,
            committed_view: &view,
            current_turn: 1,
        })
        .unwrap();
    assert_eq!(projection.plan.active_nodes, vec![node_key.clone()]);
    assert_eq!(projection.plan.active_directions.len(), 1);
    assert_eq!(projection.plan.active_directions[0].source_node, node_key);
}

#[test]
fn semantic_leaves_are_deduplicated_across_frontier_conditions() {
    let entry = NarrativeNodeKey::try_new("node.entry").unwrap();
    let key = NarrativeConditionKey::try_new("condition.stranger_trusts_you").unwrap();
    let mut node = simple_node(None);
    node.activate_when = NarrativeCondition::All {
        conditions: vec![
            NarrativeCondition::Semantic {
                semantic: SemanticNarrativeCondition {
                    condition_key: key.clone(),
                    criterion: BoundedText::try_new("does the stranger trust you", "criterion", 256).unwrap(),
                },
            },
            NarrativeCondition::Semantic {
                semantic: SemanticNarrativeCondition {
                    condition_key: key.clone(),
                    criterion: BoundedText::try_new("does the stranger trust you", "criterion", 256).unwrap(),
                },
            },
        ],
    };
    let mut nodes = BTreeMap::new();
    nodes.insert(entry.clone(), node);
    let definition = NarrativeGraphDefinition {
        entry_nodes: vec![entry],
        nodes,
        edges: Vec::new(),
    };
    let state = NarrativeRuntimeState::initial();
    let view = StubStateView;
    let projector = NarrativeProjector::new(limits());
    let projection = projector
        .project(NarrativeProjectionInput {
            definition: &definition,
            state: &state,
            committed_view: &view,
            current_turn: 1,
        })
        .unwrap();
    assert_eq!(projection.condition_queries.len(), 1);
    assert_eq!(projection.condition_queries[0].condition_key, key);
}

#[test]
fn expired_pending_effect_is_marked_not_applicable() {
    let entry = NarrativeNodeKey::try_new("node.entry").unwrap();
    let mut nodes = BTreeMap::new();
    nodes.insert(entry.clone(), simple_node(None));
    let definition = NarrativeGraphDefinition {
        entry_nodes: vec![entry.clone()],
        nodes,
        edges: Vec::new(),
    };
    let mut state = NarrativeRuntimeState::initial();
    let effect_id = NarrativeEffectId::for_transition(&entry, NarrativeTransitionKind::Activate, 1, 0);
    state.pending_effects.insert(
        effect_id.clone(),
        PendingNarrativeEffect {
            effect_id: effect_id.clone(),
            source_node: entry.clone(),
            source_transition: NarrativeTransitionKind::Activate,
            source_graph_revision: 1,
            created_by_turn: None,
            effect_index: 0,
            expires_after_turn: Some(1),
            definition: NarrativeEffectDefinition::WorldEvent(WorldEventIntentDefinition {
                event_key: CanonicalEventKey::try_new("event.example").unwrap(),
                category: BoundedText::try_new("category", "category", 128).unwrap(),
                participants: Vec::<KnowledgeEntity>::new(),
                location: None,
                description: BoundedText::try_new("something happens", "description", 256).unwrap(),
            }),
        },
    );
    let view = StubStateView;
    let projector = NarrativeProjector::new(limits());
    let projection = projector
        .project(NarrativeProjectionInput {
            definition: &definition,
            state: &state,
            committed_view: &view,
            current_turn: 2,
        })
        .unwrap();
    assert_eq!(projection.plan.effect_dispositions.len(), 1);
    assert!(matches!(
        &projection.plan.effect_dispositions[0],
        NarrativeEffectDisposition::NotApplicable { .. }
    ));
    assert!(projection.plan.world_event_intents.is_empty());
}
