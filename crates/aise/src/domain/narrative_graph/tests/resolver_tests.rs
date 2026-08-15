use crate::domain::asset::ids::{FactKey, NarrativeNodeKey, Sha256Digest};
use crate::domain::asset::validation::BoundedText;
use crate::domain::asset::validation::ScalarValue;
use crate::domain::ids::RoleId;
use crate::domain::narrative_graph::condition::RoleControllerKind;
use crate::domain::narrative_graph::condition::{NarrativeCondition, NarrativeNodeState};
use crate::domain::narrative_graph::definition::{
    NarrativeGraphDefinition, NarrativeLimits, NarrativeNodeDefinition, NarrativeNodeEffects,
};
use crate::domain::narrative_graph::resolver::{NarrativeResolutionInput, NarrativeResolver};
use crate::domain::narrative_graph::state::NarrativeRuntimeState;
use crate::domain::narrative_graph::state_view::{NarrativeStateView, NarrativeStateViewError};
use crate::domain::story_instance::state::CurrentScene;
use crate::domain::turn::{StoryCandidateVersion, StoryStateExtractionEnvelope, StoryStateExtractorOutput};
use std::collections::BTreeMap;

struct StubStateView;

impl NarrativeStateView for StubStateView {
    fn fact_value(&self, _fact_key: &FactKey) -> Result<Option<&ScalarValue>, NarrativeStateViewError> {
        Ok(None)
    }
    fn role_attribute(
        &self,
        _role_id: &RoleId,
        _attribute: &BoundedText,
    ) -> Result<Option<&ScalarValue>, NarrativeStateViewError> {
        Ok(None)
    }
    fn relationship_trust(
        &self,
        _source_role_id: &RoleId,
        _target_role_id: &RoleId,
    ) -> Result<Option<i16>, NarrativeStateViewError> {
        Ok(None)
    }
    fn role_controller(&self, _role_id: &RoleId) -> Result<RoleControllerKind, NarrativeStateViewError> {
        Ok(RoleControllerKind::Ai)
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

fn empty_extraction(expected_graph_revision: u64) -> StoryStateExtractionEnvelope {
    StoryStateExtractionEnvelope {
        candidate_version: StoryCandidateVersion {
            content_digest: Sha256Digest::from_bytes([0u8; 32]),
            repair_attempt: 0,
        },
        expected_graph_revision,
        state: StoryStateExtractorOutput {
            role_states: Vec::new(),
            relationship_states: Vec::new(),
            knowledge_changes: Vec::new(),
            current_scene: CurrentScene {
                scene_key: crate::domain::asset::ids::SceneKey::try_new("scene.example").unwrap(),
                location_key: crate::domain::asset::ids::LocationKey::try_new("location.example").unwrap(),
                time: BoundedText::try_new("morning", "time", 64).unwrap(),
                description: BoundedText::try_new("a quiet room", "description", 256).unwrap(),
                present_role_ids: Vec::new(),
            },
        },
        narrative_condition_results: Vec::new(),
    }
}

#[test]
fn entry_node_activates_when_deterministic_condition_is_satisfied() {
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
        entry_nodes: vec![entry.clone()],
        nodes,
        edges: Vec::new(),
    };
    let state = NarrativeRuntimeState::initial();
    let view = StubStateView;
    let resolver = NarrativeResolver::new(limits());
    let extraction = empty_extraction(state.graph_revision);
    let resolution = resolver
        .resolve(NarrativeResolutionInput {
            definition: &definition,
            state: &state,
            candidate_view: &view,
            extraction: &extraction,
            current_turn: 1,
        })
        .unwrap();
    assert_eq!(resolution.transitions.len(), 1);
    assert_eq!(resolution.transitions[0].node_key, entry);
    assert_eq!(resolution.next_graph_revision, 1);
}

#[test]
fn graph_revision_mismatch_is_rejected() {
    let definition = NarrativeGraphDefinition {
        entry_nodes: Vec::new(),
        nodes: BTreeMap::new(),
        edges: Vec::new(),
    };
    let state = NarrativeRuntimeState::initial();
    let view = StubStateView;
    let resolver = NarrativeResolver::new(limits());
    let extraction = empty_extraction(state.graph_revision + 1);
    let result = resolver.resolve(NarrativeResolutionInput {
        definition: &definition,
        state: &state,
        candidate_view: &view,
        extraction: &extraction,
        current_turn: 1,
    });
    assert!(result.is_err());
}

#[test]
fn active_node_completes_when_condition_satisfied() {
    let node_key = NarrativeNodeKey::try_new("node.active").unwrap();
    let mut nodes = BTreeMap::new();
    nodes.insert(
        node_key.clone(),
        NarrativeNodeDefinition {
            title: BoundedText::try_new("Active", "title", 64).unwrap(),
            dramatic_focus: None,
            activate_when: NarrativeCondition::StoryStarted,
            complete_when: NarrativeCondition::TurnReaches { turn: 2 },
            skip_when: None,
            effects: NarrativeNodeEffects {
                on_activate: Vec::new(),
                on_complete: Vec::new(),
            },
            terminal: false,
        },
    );
    let definition = NarrativeGraphDefinition {
        entry_nodes: Vec::new(),
        nodes,
        edges: Vec::new(),
    };
    let mut state = NarrativeRuntimeState::initial();
    state.node_states.insert(node_key.clone(), NarrativeNodeState::Active);
    let view = StubStateView;
    let resolver = NarrativeResolver::new(limits());
    let extraction = empty_extraction(state.graph_revision);
    let resolution = resolver
        .resolve(NarrativeResolutionInput {
            definition: &definition,
            state: &state,
            candidate_view: &view,
            extraction: &extraction,
            current_turn: 2,
        })
        .unwrap();
    assert_eq!(resolution.transitions.len(), 1);
    assert_eq!(
        resolution.transitions[0].kind,
        crate::domain::narrative_graph::effect::NarrativeTransitionKind::Complete
    );
}
