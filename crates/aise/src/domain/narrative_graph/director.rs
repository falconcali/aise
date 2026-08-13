use crate::domain::asset::ids::NarrativeNodeKey;
use crate::domain::asset::validation::BoundedText;
use crate::domain::narrative_graph::definition::{
    NarrativeCondition, NarrativeGraphDefinition, NarrativeNodeState, RoleControllerKind,
};
use crate::domain::narrative_graph::effect::{CharacterImpulse, GlobalEventIntent};
use crate::domain::narrative_graph::state::NarrativeRuntimeState;
use crate::domain::story_instance::snapshot::StoryReadSnapshot;
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct NarrativeLimits {
    pub max_nodes: usize,
    pub max_edges: usize,
    pub max_condition_depth: usize,
    pub max_conditions_per_node: usize,
    pub max_effects_per_node: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum NarrativeError {
    #[error("narrative reference is missing: {key}")]
    MissingReference { key: String },
    #[error("narrative condition limit exceeded")]
    ConditionLimitExceeded,
    #[error("narrative invariant violated: {code}")]
    Invariant { code: &'static str },
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryGoal {
    pub summary: BoundedText,
}

#[derive(Debug, Clone, Serialize)]
pub struct NarrativePlan {
    pub active_nodes: Vec<NarrativeNodeKey>,
    pub active_goals: Vec<StoryGoal>,
    pub global_event_intents: Vec<GlobalEventIntent>,
    pub character_impulses: Vec<CharacterImpulse>,
    pub proposed_transitions: Vec<ProposedNarrativeTransition>,
    pub effect_dispositions: Vec<NarrativeEffectDisposition>,
}

impl NarrativePlan {
    pub fn empty() -> Self {
        Self {
            active_nodes: Vec::new(),
            active_goals: Vec::new(),
            global_event_intents: Vec::new(),
            character_impulses: Vec::new(),
            proposed_transitions: Vec::new(),
            effect_dispositions: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ProposedNarrativeTransition {
    pub node_key: NarrativeNodeKey,
    pub from: NarrativeNodeState,
    pub to: NarrativeNodeState,
    pub expected_graph_revision: u64,
}

#[derive(Debug, Clone, Serialize)]
pub enum NarrativeEffectDisposition {
    Pending,
    NotApplicable(NotApplicableReason),
}

#[derive(Debug, Clone, Serialize)]
pub enum NotApplicableReason {
    PlayerControlled,
}

pub struct NarrativeEvaluation<'a> {
    pub definition: &'a NarrativeGraphDefinition,
    pub state: &'a NarrativeRuntimeState,
    pub snapshot: &'a StoryReadSnapshot,
}

pub struct NarrativeDirector {
    limits: NarrativeLimits,
}

impl NarrativeDirector {
    pub fn new(limits: NarrativeLimits) -> Self {
        Self { limits }
    }

    pub fn limits(&self) -> &NarrativeLimits {
        &self.limits
    }

    pub fn evaluate(&self, input: NarrativeEvaluation<'_>) -> Result<NarrativePlan, NarrativeError> {
        let definition = input.definition;
        let state = input.state;
        let snapshot = input.snapshot;
        if definition.nodes.len() > self.limits.max_nodes {
            return Err(NarrativeError::Invariant {
                code: "max_nodes_exceeded",
            });
        }
        if definition.edges.len() > self.limits.max_edges {
            return Err(NarrativeError::Invariant {
                code: "max_edges_exceeded",
            });
        }
        let turn_number = snapshot.base_revision().get();
        let mut plan = NarrativePlan::empty();
        for (node_key, node_def) in &definition.nodes {
            let current = state.node_state(node_key);
            match current {
                NarrativeNodeState::Inactive => {
                    if self.matches(&node_def.activate_when, state, snapshot, turn_number, 0)? {
                        plan.active_nodes.push(node_key.clone());
                        plan.proposed_transitions.push(ProposedNarrativeTransition {
                            node_key: node_key.clone(),
                            from: NarrativeNodeState::Inactive,
                            to: NarrativeNodeState::Active,
                            expected_graph_revision: state.graph_revision,
                        });
                        plan.effect_dispositions.push(NarrativeEffectDisposition::Pending);
                        plan.active_goals.push(StoryGoal {
                            summary: node_def.objective.clone(),
                        });
                    }
                }
                NarrativeNodeState::Active => {
                    if self.matches(&node_def.complete_when, state, snapshot, turn_number, 0)? {
                        plan.proposed_transitions.push(ProposedNarrativeTransition {
                            node_key: node_key.clone(),
                            from: NarrativeNodeState::Active,
                            to: NarrativeNodeState::Completed,
                            expected_graph_revision: state.graph_revision,
                        });
                        plan.effect_dispositions.push(NarrativeEffectDisposition::Pending);
                    } else if let Some(skip_when) = &node_def.skip_when {
                        if self.matches(skip_when, state, snapshot, turn_number, 0)? {
                            plan.proposed_transitions.push(ProposedNarrativeTransition {
                                node_key: node_key.clone(),
                                from: NarrativeNodeState::Active,
                                to: NarrativeNodeState::Skipped,
                                expected_graph_revision: state.graph_revision,
                            });
                        }
                    }
                }
                _ => {}
            }
        }
        for edge in &definition.edges {
            let from_state = state.node_state(&edge.from);
            if from_state != NarrativeNodeState::Active {
                continue;
            }
            if self.matches(&edge.when, state, snapshot, turn_number, 0)? {
                plan.active_nodes.push(edge.to.clone());
            }
        }
        for node_key in &plan.active_nodes {
            for effect in &definition
                .nodes
                .get(node_key)
                .map(|node| &node.effects.on_activate)
                .cloned()
                .unwrap_or_default()
            {
                match effect {
                    crate::domain::narrative_graph::effect::NarrativeEffectDefinition::GlobalEvent(event) => {
                        plan.global_event_intents.push(GlobalEventIntent {
                            source_node: node_key.clone(),
                            event_key: event.event_key.clone(),
                            category: event.category.clone(),
                            participants: event.participants.clone(),
                            location: event.location.clone(),
                            description: event.description.clone(),
                        });
                    }
                    crate::domain::narrative_graph::effect::NarrativeEffectDefinition::CharacterImpulse(impulse) => {
                        if let Some(binding) = snapshot.role_binding(&impulse.target_role_key) {
                            if binding.is_player_controlled() {
                                plan.effect_dispositions.push(NarrativeEffectDisposition::NotApplicable(
                                    NotApplicableReason::PlayerControlled,
                                ));
                            } else {
                                plan.character_impulses.push(CharacterImpulse {
                                    source_node: node_key.clone(),
                                    target_role_key: impulse.target_role_key.clone(),
                                    target_character_id: binding.character_id.clone(),
                                    goal: impulse.goal.clone(),
                                    reason: impulse.reason.clone(),
                                    emotion: impulse.emotion.clone(),
                                    urgency: impulse.urgency,
                                    expires_after_turn: impulse
                                        .valid_for_turns
                                        .map(|turns| turn_number.saturating_add(turns.get() as u64)),
                                });
                            }
                        }
                    }
                }
            }
        }
        Ok(plan)
    }

    fn matches(
        &self,
        condition: &NarrativeCondition,
        state: &NarrativeRuntimeState,
        snapshot: &StoryReadSnapshot,
        turn: u64,
        depth: usize,
    ) -> Result<bool, NarrativeError> {
        if depth > self.limits.max_condition_depth {
            return Err(NarrativeError::ConditionLimitExceeded);
        }
        match condition {
            NarrativeCondition::All { conditions } => {
                if conditions.len() > self.limits.max_conditions_per_node {
                    return Err(NarrativeError::ConditionLimitExceeded);
                }
                for condition in conditions {
                    if !self.matches(condition, state, snapshot, turn, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            NarrativeCondition::Any { conditions } => {
                if conditions.len() > self.limits.max_conditions_per_node {
                    return Err(NarrativeError::ConditionLimitExceeded);
                }
                for condition in conditions {
                    if self.matches(condition, state, snapshot, turn, depth + 1)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            NarrativeCondition::Not { condition } => Ok(!self.matches(condition, state, snapshot, turn, depth + 1)?),
            NarrativeCondition::StoryStarted => Ok(true),
            NarrativeCondition::TurnReaches { turn: target } => Ok(turn >= *target),
            NarrativeCondition::NodeState {
                node_key,
                state: expected,
            } => Ok(state.node_state(node_key) == *expected),
            NarrativeCondition::EventOccurred { event_key } => {
                Ok(snapshot.condition_state().occurred_event_keys.contains(event_key))
            }
            NarrativeCondition::FactStateEquals { fact_key, value } => {
                Ok(snapshot.condition_state().fact_values.get(fact_key) == Some(value))
            }
            NarrativeCondition::CharacterStateEquals {
                role_key,
                attribute,
                value,
            } => {
                let Some(binding) = snapshot.role_binding(role_key) else {
                    return Err(NarrativeError::MissingReference {
                        key: role_key.as_str().to_owned(),
                    });
                };
                let Some(character) = snapshot.character_states().get(&binding.character_id) else {
                    return Err(NarrativeError::MissingReference {
                        key: binding.character_id.as_str().to_owned(),
                    });
                };
                Ok(character
                    .attributes
                    .get(&crate::domain::asset::ids::AttributeKey::from(attribute.as_str()))
                    == Some(value))
            }
            NarrativeCondition::RelationshipReaches {
                source_role_key,
                target_role_key,
                minimum_trust,
            } => {
                let source =
                    snapshot
                        .role_binding(source_role_key)
                        .ok_or_else(|| NarrativeError::MissingReference {
                            key: source_role_key.as_str().to_owned(),
                        })?;
                let target =
                    snapshot
                        .role_binding(target_role_key)
                        .ok_or_else(|| NarrativeError::MissingReference {
                            key: target_role_key.as_str().to_owned(),
                        })?;
                Ok(snapshot.relationships().iter().any(|relationship| {
                    relationship.source_character_id == source.character_id
                        && relationship.target_character_id == target.character_id
                        && relationship.trust >= *minimum_trust
                }))
            }
            NarrativeCondition::PlayerActionOccurred { event_key } => {
                Ok(snapshot.condition_state().player_action_event_keys.contains(event_key))
            }
            NarrativeCondition::RoleControllerIs { role_key, controller } => {
                let kind = node_controller_kind(role_key, snapshot);
                Ok(kind == *controller)
            }
        }
    }
}

fn node_controller_kind(
    role_key: &crate::domain::asset::ids::StoryRoleKey,
    snapshot: &StoryReadSnapshot,
) -> RoleControllerKind {
    match snapshot.role_binding(role_key) {
        Some(binding) if binding.is_player_controlled() => RoleControllerKind::Player,
        _ => RoleControllerKind::Ai,
    }
}
