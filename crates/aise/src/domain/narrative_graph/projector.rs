use crate::domain::asset::ids::{NarrativeConditionKey, NarrativeNodeKey};
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::CharacterId;
use crate::domain::narrative_graph::condition::{NarrativeCondition, NarrativeNodeState, RoleControllerKind};
use crate::domain::narrative_graph::definition::{NarrativeError, NarrativeGraphDefinition, NarrativeLimits};
use crate::domain::narrative_graph::effect::{
    CharacterImpulse, NarrativeEffectDefinition, NarrativeEffectId, WorldEventIntent,
};
use crate::domain::narrative_graph::state::NarrativeRuntimeState;
use crate::domain::narrative_graph::state_view::NarrativeStateView;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize)]
pub struct NarrativeDirection {
    pub source_node: NarrativeNodeKey,
    pub dramatic_focus: BoundedText,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NarrativeEffectNotApplicableReason {
    PlayerControlled,
    Expired,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum NarrativeEffectDisposition {
    PendingDelivery {
        effect_id: NarrativeEffectId,
    },
    NotApplicable {
        effect_id: NarrativeEffectId,
        reason: NarrativeEffectNotApplicableReason,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct NarrativePlan {
    pub active_nodes: Vec<NarrativeNodeKey>,
    pub active_directions: Vec<NarrativeDirection>,
    pub world_event_intents: Vec<WorldEventIntent>,
    pub character_impulses: Vec<CharacterImpulse>,
    pub effect_dispositions: Vec<NarrativeEffectDisposition>,
}

impl NarrativePlan {
    pub fn empty() -> Self {
        Self {
            active_nodes: Vec::new(),
            active_directions: Vec::new(),
            world_event_intents: Vec::new(),
            character_impulses: Vec::new(),
            effect_dispositions: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct NarrativeConditionQuery {
    pub condition_key: NarrativeConditionKey,
    pub criterion: BoundedText,
}

#[derive(Debug, Clone, Serialize)]
pub struct NarrativeProjection {
    pub plan: NarrativePlan,
    pub condition_queries: Vec<NarrativeConditionQuery>,
    pub expected_graph_revision: u64,
}

pub struct NarrativeProjectionInput<'a> {
    pub definition: &'a NarrativeGraphDefinition,
    pub state: &'a NarrativeRuntimeState,
    pub committed_view: &'a dyn NarrativeStateView,
    pub current_turn: u64,
}

pub struct NarrativeProjector {
    limits: NarrativeLimits,
}

impl NarrativeProjector {
    pub fn new(limits: NarrativeLimits) -> Self {
        Self { limits }
    }

    pub fn project(&self, input: NarrativeProjectionInput<'_>) -> Result<NarrativeProjection, NarrativeError> {
        let active_nodes: Vec<NarrativeNodeKey> = input
            .state
            .node_states
            .iter()
            .filter(|(_, state)| **state == NarrativeNodeState::Active)
            .map(|(node_key, _)| node_key.clone())
            .collect();

        let active_directions = active_nodes
            .iter()
            .filter_map(|node_key| {
                input.definition.nodes.get(node_key).and_then(|node| {
                    node.dramatic_focus.clone().map(|dramatic_focus| NarrativeDirection {
                        source_node: node_key.clone(),
                        dramatic_focus,
                    })
                })
            })
            .collect();

        let mut frontier_nodes: BTreeSet<NarrativeNodeKey> = BTreeSet::new();
        let mut frontier_conditions: Vec<&NarrativeCondition> = Vec::new();

        for entry in &input.definition.entry_nodes {
            if input.state.node_state(entry) == NarrativeNodeState::Inactive {
                let node = input
                    .definition
                    .nodes
                    .get(entry)
                    .ok_or_else(|| NarrativeError::MissingReference { key: entry.to_string() })?;
                frontier_nodes.insert(entry.clone());
                frontier_conditions.push(&node.activate_when);
            }
        }

        for node_key in &active_nodes {
            let node = input
                .definition
                .nodes
                .get(node_key)
                .ok_or_else(|| NarrativeError::MissingReference {
                    key: node_key.to_string(),
                })?;
            frontier_nodes.insert(node_key.clone());
            frontier_conditions.push(&node.complete_when);
            if let Some(skip_when) = &node.skip_when {
                frontier_conditions.push(skip_when);
            }
        }

        for edge in &input.definition.edges {
            if !active_nodes.contains(&edge.from) {
                continue;
            }
            frontier_nodes.insert(edge.from.clone());
            frontier_conditions.push(&edge.when);
            if input.state.node_state(&edge.to) == NarrativeNodeState::Inactive {
                let successor =
                    input
                        .definition
                        .nodes
                        .get(&edge.to)
                        .ok_or_else(|| NarrativeError::MissingReference {
                            key: edge.to.to_string(),
                        })?;
                frontier_nodes.insert(edge.to.clone());
                frontier_conditions.push(&successor.activate_when);
            }
        }

        if frontier_nodes.len() > self.limits.max_frontier_nodes {
            return Err(NarrativeError::FrontierLimitExceeded);
        }

        let mut queries: BTreeMap<NarrativeConditionKey, BoundedText> = BTreeMap::new();
        for condition in &frontier_conditions {
            collect_semantic_leaves(condition, &mut queries)?;
        }
        if queries.len() > self.limits.max_semantic_conditions {
            return Err(NarrativeError::SemanticConditionLimitExceeded);
        }
        if queries.len() > self.limits.max_semantic_queries_per_turn {
            return Err(NarrativeError::SemanticQueryLimitExceeded);
        }
        let total_bytes: usize = queries.values().map(|criterion| criterion.as_str().len()).sum();
        if total_bytes > self.limits.max_semantic_query_bytes {
            return Err(NarrativeError::SemanticQueryByteLimitExceeded);
        }

        let condition_queries = queries
            .into_iter()
            .map(|(condition_key, criterion)| NarrativeConditionQuery {
                condition_key,
                criterion,
            })
            .collect();

        let (world_event_intents, character_impulses, effect_dispositions) =
            self.project_effects(&input, input.state)?;

        let plan = NarrativePlan {
            active_nodes,
            active_directions,
            world_event_intents,
            character_impulses,
            effect_dispositions,
        };

        Ok(NarrativeProjection {
            plan,
            condition_queries,
            expected_graph_revision: input.state.graph_revision,
        })
    }
}

type ProjectedEffects = (Vec<WorldEventIntent>, Vec<CharacterImpulse>, Vec<NarrativeEffectDisposition>);

impl NarrativeProjector {
    fn project_effects(
        &self,
        input: &NarrativeProjectionInput<'_>,
        state: &NarrativeRuntimeState,
    ) -> Result<ProjectedEffects, NarrativeError> {
        let mut world_event_intents = Vec::new();
        let mut character_impulses = Vec::new();
        let mut effect_dispositions = Vec::new();

        for (effect_id, pending) in &state.pending_effects {
            if pending
                .expires_after_turn
                .is_some_and(|expires_after_turn| expires_after_turn < input.current_turn)
            {
                effect_dispositions.push(NarrativeEffectDisposition::NotApplicable {
                    effect_id: effect_id.clone(),
                    reason: NarrativeEffectNotApplicableReason::Expired,
                });
                continue;
            }

            match &pending.definition {
                NarrativeEffectDefinition::WorldEvent(definition) => {
                    world_event_intents.push(WorldEventIntent {
                        effect_id: effect_id.clone(),
                        source_node: pending.source_node.clone(),
                        event_key: definition.event_key.clone(),
                        category: definition.category.clone(),
                        participants: definition.participants.clone(),
                        location: definition.location.clone(),
                        description: definition.description.clone(),
                    });
                    effect_dispositions.push(NarrativeEffectDisposition::PendingDelivery {
                        effect_id: effect_id.clone(),
                    });
                }
                NarrativeEffectDefinition::CharacterImpulse(definition) => {
                    let controller =
                        input.committed_view.role_controller(&definition.target_role_key).map_err(|_| {
                            NarrativeError::Invariant {
                                code: "unknown_character_impulse_role_reference",
                            }
                        })?;
                    if controller == RoleControllerKind::Player {
                        effect_dispositions.push(NarrativeEffectDisposition::NotApplicable {
                            effect_id: effect_id.clone(),
                            reason: NarrativeEffectNotApplicableReason::PlayerControlled,
                        });
                        continue;
                    }
                    let target_character_id: CharacterId = input
                        .committed_view
                        .character_id_for_role(&definition.target_role_key)
                        .map_err(|_| NarrativeError::Invariant {
                            code: "unknown_character_impulse_role_reference",
                        })?;
                    character_impulses.push(CharacterImpulse {
                        effect_id: effect_id.clone(),
                        source_node: pending.source_node.clone(),
                        target_role_key: definition.target_role_key.clone(),
                        target_character_id,
                        goal: definition.goal.clone(),
                        reason: definition.reason.clone(),
                        emotion: definition.emotion.clone(),
                        urgency: definition.urgency,
                        expires_after_turn: pending.expires_after_turn,
                    });
                    effect_dispositions.push(NarrativeEffectDisposition::PendingDelivery {
                        effect_id: effect_id.clone(),
                    });
                }
            }
        }

        Ok((world_event_intents, character_impulses, effect_dispositions))
    }
}

fn collect_semantic_leaves(
    condition: &NarrativeCondition,
    queries: &mut BTreeMap<NarrativeConditionKey, BoundedText>,
) -> Result<(), NarrativeError> {
    match condition {
        NarrativeCondition::All { conditions } | NarrativeCondition::Any { conditions } => {
            for condition in conditions {
                collect_semantic_leaves(condition, queries)?;
            }
            Ok(())
        }
        NarrativeCondition::Not { condition } => collect_semantic_leaves(condition, queries),
        NarrativeCondition::Semantic { semantic } => {
            if let Some(existing) = queries.get(&semantic.condition_key) {
                if existing != &semantic.criterion {
                    return Err(NarrativeError::SemanticCriterionConflict {
                        key: semantic.condition_key.to_string(),
                    });
                }
            } else {
                queries.insert(semantic.condition_key.clone(), semantic.criterion.clone());
            }
            Ok(())
        }
        NarrativeCondition::StoryStarted
        | NarrativeCondition::NodeState { .. }
        | NarrativeCondition::FactStateEquals { .. }
        | NarrativeCondition::CharacterStateEquals { .. }
        | NarrativeCondition::RelationshipReaches { .. }
        | NarrativeCondition::TurnReaches { .. }
        | NarrativeCondition::RoleControllerIs { .. } => Ok(()),
    }
}

#[cfg(test)]
#[path = "tests/projector_tests.rs"]
mod tests;
