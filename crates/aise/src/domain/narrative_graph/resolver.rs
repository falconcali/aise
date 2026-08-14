use crate::domain::asset::ids::{NarrativeConditionKey, NarrativeNodeKey};
use crate::domain::narrative_graph::condition::{
    ConditionEvalContext, NarrativeNodeState, NarrativeTruthValue, evaluate_condition,
};
use crate::domain::narrative_graph::definition::{NarrativeError, NarrativeGraphDefinition, NarrativeLimits};
use crate::domain::narrative_graph::effect::{NarrativeEffectId, NarrativeTransitionKind};
use crate::domain::narrative_graph::state::{NarrativeRuntimeState, PendingNarrativeEffect};
use crate::domain::narrative_graph::state_view::NarrativeStateView;
use crate::domain::turn::extraction::{NarrativeConditionStatus, StoryStateExtractionEnvelope};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
pub struct ProposedNarrativeTransition {
    pub node_key: NarrativeNodeKey,
    pub kind: NarrativeTransitionKind,
}

#[derive(Debug, Clone, Serialize)]
pub struct NarrativeResolution {
    pub transitions: Vec<ProposedNarrativeTransition>,
    pub condition_results: BTreeMap<NarrativeConditionKey, NarrativeTruthValue>,
    pub pending_effects: Vec<PendingNarrativeEffect>,
    pub next_graph_revision: u64,
}

pub struct NarrativeResolutionInput<'a> {
    pub definition: &'a NarrativeGraphDefinition,
    pub state: &'a NarrativeRuntimeState,
    pub candidate_view: &'a dyn NarrativeStateView,
    pub extraction: &'a StoryStateExtractionEnvelope,
    pub current_turn: u64,
}

pub struct NarrativeResolver {
    limits: NarrativeLimits,
}

impl NarrativeResolver {
    pub fn new(limits: NarrativeLimits) -> Self {
        Self { limits }
    }

    pub fn resolve(&self, input: NarrativeResolutionInput<'_>) -> Result<NarrativeResolution, NarrativeError> {
        if input.extraction.expected_graph_revision != input.state.graph_revision {
            return Err(NarrativeError::GraphRevisionMismatch);
        }

        let semantic_results: BTreeMap<NarrativeConditionKey, NarrativeTruthValue> = input
            .extraction
            .narrative_condition_results
            .iter()
            .map(|result| (result.condition_key.clone(), result.status.into()))
            .collect();

        let eval_ctx = ConditionEvalContext {
            state: input.state,
            view: input.candidate_view,
            semantic_results: &semantic_results,
            current_turn: input.current_turn,
            limits: self.limits,
        };

        let mut next_states: BTreeMap<NarrativeNodeKey, NarrativeNodeState> = BTreeMap::new();
        let mut transitions: Vec<ProposedNarrativeTransition> = Vec::new();
        let mut activated_this_turn: Vec<NarrativeNodeKey> = Vec::new();

        for entry in &input.definition.entry_nodes {
            if input.state.node_state(entry) != NarrativeNodeState::Inactive {
                continue;
            }
            let node = input
                .definition
                .nodes
                .get(entry)
                .ok_or_else(|| NarrativeError::MissingReference { key: entry.to_string() })?;
            if evaluate_condition(&node.activate_when, &eval_ctx, 0)? == NarrativeTruthValue::Satisfied {
                next_states.insert(entry.clone(), NarrativeNodeState::Active);
                transitions.push(ProposedNarrativeTransition {
                    node_key: entry.clone(),
                    kind: NarrativeTransitionKind::Activate,
                });
                activated_this_turn.push(entry.clone());
            }
        }

        let active_at_turn_start: Vec<NarrativeNodeKey> = input
            .state
            .node_states
            .iter()
            .filter(|(_, state)| **state == NarrativeNodeState::Active)
            .map(|(node_key, _)| node_key.clone())
            .collect();

        for node_key in &active_at_turn_start {
            let node = input
                .definition
                .nodes
                .get(node_key)
                .ok_or_else(|| NarrativeError::MissingReference {
                    key: node_key.to_string(),
                })?;

            if let Some(skip_when) = &node.skip_when {
                if evaluate_condition(skip_when, &eval_ctx, 0)? == NarrativeTruthValue::Satisfied {
                    next_states.insert(node_key.clone(), NarrativeNodeState::Skipped);
                    transitions.push(ProposedNarrativeTransition {
                        node_key: node_key.clone(),
                        kind: NarrativeTransitionKind::Skip,
                    });
                    continue;
                }
            }

            if evaluate_condition(&node.complete_when, &eval_ctx, 0)? == NarrativeTruthValue::Satisfied {
                next_states.insert(node_key.clone(), NarrativeNodeState::Completed);
                transitions.push(ProposedNarrativeTransition {
                    node_key: node_key.clone(),
                    kind: NarrativeTransitionKind::Complete,
                });
            }
        }

        for edge in &input.definition.edges {
            if !active_at_turn_start.contains(&edge.from) {
                continue;
            }
            if input.state.node_state(&edge.to) != NarrativeNodeState::Inactive {
                continue;
            }
            if next_states.contains_key(&edge.to) {
                continue;
            }
            if evaluate_condition(&edge.when, &eval_ctx, 0)? == NarrativeTruthValue::Satisfied {
                next_states.insert(edge.to.clone(), NarrativeNodeState::Active);
                transitions.push(ProposedNarrativeTransition {
                    node_key: edge.to.clone(),
                    kind: NarrativeTransitionKind::Activate,
                });
                activated_this_turn.push(edge.to.clone());
            }
        }

        if transitions.len() > self.limits.max_transitions_per_turn {
            return Err(NarrativeError::TransitionLimitExceeded);
        }

        let mut pending_effects: Vec<PendingNarrativeEffect> = Vec::new();
        let next_graph_revision = input.state.graph_revision + 1;
        for transition in &transitions {
            let node =
                input
                    .definition
                    .nodes
                    .get(&transition.node_key)
                    .ok_or_else(|| NarrativeError::MissingReference {
                        key: transition.node_key.to_string(),
                    })?;
            let definitions = match transition.kind {
                NarrativeTransitionKind::Activate => &node.effects.on_activate,
                NarrativeTransitionKind::Complete => &node.effects.on_complete,
                NarrativeTransitionKind::Skip => continue,
            };
            for (effect_index, definition) in definitions.iter().enumerate() {
                let effect_id = NarrativeEffectId::for_transition(
                    &transition.node_key,
                    transition.kind,
                    next_graph_revision,
                    effect_index as u32,
                );
                pending_effects.push(PendingNarrativeEffect {
                    effect_id,
                    source_node: transition.node_key.clone(),
                    source_transition: transition.kind,
                    source_graph_revision: next_graph_revision,
                    created_by_turn: None,
                    effect_index: effect_index as u32,
                    expires_after_turn: None,
                    definition: definition.clone(),
                });
            }
        }

        let total_pending = input.state.pending_effects.len() + pending_effects.len();
        if total_pending > self.limits.max_pending_effects {
            return Err(NarrativeError::PendingEffectLimitExceeded);
        }

        Ok(NarrativeResolution {
            transitions,
            condition_results: semantic_results,
            pending_effects,
            next_graph_revision,
        })
    }
}

impl From<NarrativeConditionStatus> for NarrativeTruthValue {
    fn from(status: NarrativeConditionStatus) -> Self {
        match status {
            NarrativeConditionStatus::Satisfied => Self::Satisfied,
            NarrativeConditionStatus::Unsatisfied => Self::Unsatisfied,
            NarrativeConditionStatus::Unknown => Self::Unknown,
        }
    }
}

#[cfg(test)]
#[path = "tests/resolver_tests.rs"]
mod tests;
