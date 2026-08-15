use crate::domain::asset::ids::{FactKey, NarrativeConditionKey, NarrativeNodeKey};
use crate::domain::asset::validation::{BoundedText, ScalarValue};
use crate::domain::ids::RoleId;
use crate::domain::narrative_graph::definition::{NarrativeError, NarrativeLimits};
use crate::domain::narrative_graph::state::NarrativeRuntimeState;
use crate::domain::narrative_graph::state_view::NarrativeStateView;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NarrativeNodeState {
    Inactive,
    Active,
    Completed,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleControllerKind {
    Player,
    Ai,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NarrativeTruthValue {
    Satisfied,
    Unsatisfied,
    Unknown,
}

impl NarrativeTruthValue {
    fn not(self) -> Self {
        match self {
            Self::Satisfied => Self::Unsatisfied,
            Self::Unsatisfied => Self::Satisfied,
            Self::Unknown => Self::Unknown,
        }
    }

    fn all(values: &[Self]) -> Self {
        if values.contains(&Self::Unsatisfied) {
            return Self::Unsatisfied;
        }
        if values.iter().all(|value| *value == Self::Satisfied) {
            return Self::Satisfied;
        }
        Self::Unknown
    }

    fn any(values: &[Self]) -> Self {
        if values.contains(&Self::Satisfied) {
            return Self::Satisfied;
        }
        if values.iter().all(|value| *value == Self::Unsatisfied) {
            return Self::Unsatisfied;
        }
        Self::Unknown
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticNarrativeCondition {
    pub condition_key: NarrativeConditionKey,
    pub criterion: BoundedText,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum NarrativeCondition {
    All {
        conditions: Vec<NarrativeCondition>,
    },
    Any {
        conditions: Vec<NarrativeCondition>,
    },
    Not {
        condition: Box<NarrativeCondition>,
    },
    StoryStarted,
    NodeState {
        node_key: NarrativeNodeKey,
        state: NarrativeNodeState,
    },
    Semantic {
        #[serde(flatten)]
        semantic: SemanticNarrativeCondition,
    },
    FactStateEquals {
        fact_key: FactKey,
        value: ScalarValue,
    },
    RoleStateEquals {
        role_id: RoleId,
        attribute: BoundedText,
        value: ScalarValue,
    },
    RelationshipReaches {
        source_role_id: RoleId,
        target_role_id: RoleId,
        minimum_trust: i16,
    },
    TurnReaches {
        turn: u64,
    },
    RoleControllerIs {
        role_id: RoleId,
        controller: RoleControllerKind,
    },
}

pub struct ConditionEvalContext<'a> {
    pub state: &'a NarrativeRuntimeState,
    pub view: &'a dyn NarrativeStateView,
    pub semantic_results: &'a BTreeMap<NarrativeConditionKey, NarrativeTruthValue>,
    pub current_turn: u64,
    pub limits: NarrativeLimits,
}

pub fn evaluate_condition(
    condition: &NarrativeCondition,
    ctx: &ConditionEvalContext<'_>,
    depth: usize,
) -> Result<NarrativeTruthValue, NarrativeError> {
    if depth > ctx.limits.max_condition_depth {
        return Err(NarrativeError::ConditionDepthExceeded);
    }
    match condition {
        NarrativeCondition::All { conditions } => {
            if conditions.is_empty() || conditions.len() > ctx.limits.max_conditions_per_node {
                return Err(NarrativeError::ConditionCountExceeded);
            }
            let values = conditions
                .iter()
                .map(|condition| evaluate_condition(condition, ctx, depth + 1))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(NarrativeTruthValue::all(&values))
        }
        NarrativeCondition::Any { conditions } => {
            if conditions.is_empty() || conditions.len() > ctx.limits.max_conditions_per_node {
                return Err(NarrativeError::ConditionCountExceeded);
            }
            let values = conditions
                .iter()
                .map(|condition| evaluate_condition(condition, ctx, depth + 1))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(NarrativeTruthValue::any(&values))
        }
        NarrativeCondition::Not { condition } => Ok(evaluate_condition(condition, ctx, depth + 1)?.not()),
        NarrativeCondition::StoryStarted => Ok(NarrativeTruthValue::Satisfied),
        NarrativeCondition::TurnReaches { turn } => Ok(if ctx.current_turn >= *turn {
            NarrativeTruthValue::Satisfied
        } else {
            NarrativeTruthValue::Unsatisfied
        }),
        NarrativeCondition::NodeState { node_key, state } => Ok(if ctx.state.node_state(node_key) == *state {
            NarrativeTruthValue::Satisfied
        } else {
            NarrativeTruthValue::Unsatisfied
        }),
        NarrativeCondition::Semantic { semantic } => Ok(ctx
            .semantic_results
            .get(&semantic.condition_key)
            .copied()
            .unwrap_or(NarrativeTruthValue::Unknown)),
        NarrativeCondition::FactStateEquals { fact_key, value } => {
            let current = ctx.view.fact_value(fact_key).map_err(|_| NarrativeError::Invariant {
                code: "unknown_fact_reference",
            })?;
            Ok(match current {
                Some(current) if current == value => NarrativeTruthValue::Satisfied,
                _ => NarrativeTruthValue::Unsatisfied,
            })
        }
        NarrativeCondition::RoleStateEquals {
            role_id,
            attribute,
            value,
        } => {
            let current = ctx
                .view
                .role_attribute(role_id, attribute)
                .map_err(|_| NarrativeError::Invariant {
                    code: "unknown_role_state_reference",
                })?;
            Ok(match current {
                Some(current) if current == value => NarrativeTruthValue::Satisfied,
                _ => NarrativeTruthValue::Unsatisfied,
            })
        }
        NarrativeCondition::RelationshipReaches {
            source_role_id,
            target_role_id,
            minimum_trust,
        } => {
            let trust =
                ctx.view
                    .relationship_trust(source_role_id, target_role_id)
                    .map_err(|_| NarrativeError::Invariant {
                        code: "unknown_relationship_role_reference",
                    })?;
            Ok(match trust {
                Some(trust) if trust >= *minimum_trust => NarrativeTruthValue::Satisfied,
                _ => NarrativeTruthValue::Unsatisfied,
            })
        }
        NarrativeCondition::RoleControllerIs { role_id, controller } => {
            let current = ctx.view.role_controller(role_id).map_err(|_| NarrativeError::Invariant {
                code: "unknown_role_controller_reference",
            })?;
            Ok(if current == *controller {
                NarrativeTruthValue::Satisfied
            } else {
                NarrativeTruthValue::Unsatisfied
            })
        }
    }
}

#[cfg(test)]
#[path = "tests/condition_tests.rs"]
mod tests;
