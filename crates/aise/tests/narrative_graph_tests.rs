use aise::domain::asset::ids::NarrativeNodeKey;
use aise::domain::asset::validation::BoundedText;
use aise::domain::narrative_graph::condition::NarrativeCondition;
use aise::domain::narrative_graph::definition::{
    NarrativeGraphDefinition, NarrativeLimits, NarrativeNodeDefinition, NarrativeNodeEffects,
};
use aise::domain::narrative_graph::projector::{NarrativeProjectionInput, NarrativeProjector};
use aise::domain::narrative_graph::state::NarrativeRuntimeState;
use aise::domain::narrative_graph::state_view::{NarrativeStateView, NarrativeStateViewError};
use std::collections::BTreeMap;

struct EmptyStateView;

impl NarrativeStateView for EmptyStateView {
    fn fact_value(
        &self,
        _fact_key: &aise::domain::asset::ids::FactKey,
    ) -> Result<Option<&aise::domain::asset::validation::ScalarValue>, NarrativeStateViewError> {
        Ok(None)
    }
    fn role_attribute(
        &self,
        _role_id: &aise::domain::ids::RoleId,
        _attribute: &aise::domain::asset::validation::BoundedText,
    ) -> Result<Option<&aise::domain::asset::validation::ScalarValue>, NarrativeStateViewError> {
        Ok(None)
    }
    fn relationship_trust(
        &self,
        _source_role_id: &aise::domain::ids::RoleId,
        _target_role_id: &aise::domain::ids::RoleId,
    ) -> Result<Option<i16>, NarrativeStateViewError> {
        Ok(None)
    }
    fn role_controller(
        &self,
        _role_id: &aise::domain::ids::RoleId,
    ) -> Result<aise::domain::narrative_graph::condition::RoleControllerKind, NarrativeStateViewError> {
        Ok(aise::domain::narrative_graph::condition::RoleControllerKind::Ai)
    }
}

fn limits() -> NarrativeLimits {
    NarrativeLimits {
        max_graph_nodes: 32,
        max_graph_edges: 64,
        max_condition_depth: 8,
        max_conditions_per_node: 8,
        max_effects_per_node: 8,
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

#[test]
fn narrative_projector_produces_an_empty_plan_for_an_empty_graph() {
    let definition = NarrativeGraphDefinition {
        entry_nodes: Vec::new(),
        nodes: BTreeMap::new(),
        edges: Vec::new(),
    };
    let state = NarrativeRuntimeState::initial();
    let view = EmptyStateView;
    let projector = NarrativeProjector::new(limits());
    let projection = projector
        .project(NarrativeProjectionInput {
            definition: &definition,
            state: &state,
            committed_view: &view,
            current_turn: 1,
        })
        .expect("projection should succeed on an empty graph");
    assert!(projection.plan.active_nodes.is_empty());
    assert!(projection.plan.character_impulses.is_empty());
    assert!(projection.condition_queries.is_empty());
}

#[test]
fn narrative_projector_includes_entry_node_activation_frontier() {
    let entry = NarrativeNodeKey::try_new("node.entry").unwrap();
    let mut nodes = BTreeMap::new();
    nodes.insert(
        entry.clone(),
        NarrativeNodeDefinition {
            title: BoundedText::try_new("Entry", "title", 64).unwrap(),
            dramatic_focus: None,
            activate_when: NarrativeCondition::StoryStarted,
            complete_when: NarrativeCondition::TurnReaches { turn: 999 },
            skip_when: None,
            effects: NarrativeNodeEffects {
                on_activate: Vec::new(),
                on_complete: Vec::new(),
            },
            terminal: false,
        },
    );
    let definition = NarrativeGraphDefinition {
        entry_nodes: vec![entry],
        nodes,
        edges: Vec::new(),
    };
    let state = NarrativeRuntimeState::initial();
    let view = EmptyStateView;
    let projector = NarrativeProjector::new(limits());
    let projection = projector
        .project(NarrativeProjectionInput {
            definition: &definition,
            state: &state,
            committed_view: &view,
            current_turn: 1,
        })
        .expect("projection should succeed");
    assert!(projection.plan.active_nodes.is_empty());
    assert_eq!(projection.expected_graph_revision, 0);
}
