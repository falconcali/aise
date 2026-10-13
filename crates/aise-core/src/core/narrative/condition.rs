use crate::core::narrative::{NarrativeError, NarrativeNode, NarrativeState, NodeStatus};
use crate::core::{BoundedText, NarrativeConditionId, NarrativeNodeId, TurnNumber};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Truth {
    Satisfied,
    Unsatisfied,
    Unknown,
}

impl Truth {
    fn from_bool(value: bool) -> Self {
        if value { Self::Satisfied } else { Self::Unsatisfied }
    }

    fn negate(self) -> Self {
        match self {
            Self::Satisfied => Self::Unsatisfied,
            Self::Unsatisfied => Self::Satisfied,
            Self::Unknown => Self::Unknown,
        }
    }

    fn all(values: impl IntoIterator<Item = Self>) -> Self {
        let mut result = Self::Satisfied;
        for value in values {
            match value {
                Self::Unsatisfied => return Self::Unsatisfied,
                Self::Unknown => result = Self::Unknown,
                Self::Satisfied => {}
            }
        }
        result
    }

    fn any(values: impl IntoIterator<Item = Self>) -> Self {
        let mut result = Self::Unsatisfied;
        for value in values {
            match value {
                Self::Satisfied => return Self::Satisfied,
                Self::Unknown => result = Self::Unknown,
                Self::Unsatisfied => {}
            }
        }
        result
    }
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
    TurnReaches {
        turn: u64,
    },
    NodeIs {
        node_id: NarrativeNodeId,
        status: NodeStatus,
    },
    Semantic {
        condition_id: NarrativeConditionId,
        criterion: BoundedText,
    },
}

pub struct NarrativeEvidence<'a> {
    pub turn: TurnNumber,
    pub semantic_results: &'a BTreeMap<NarrativeConditionId, Truth>,
}

impl NarrativeCondition {
    pub fn evaluate(&self, state: &NarrativeState, input: &NarrativeEvidence<'_>) -> Truth {
        match self {
            Self::All { conditions } => Truth::all(conditions.iter().map(|c| c.evaluate(state, input))),
            Self::Any { conditions } => Truth::any(conditions.iter().map(|c| c.evaluate(state, input))),
            Self::Not { condition } => condition.evaluate(state, input).negate(),
            Self::StoryStarted => Truth::Satisfied,
            Self::TurnReaches { turn } => Truth::from_bool(input.turn.value() >= *turn),
            Self::NodeIs { node_id, status } => Truth::from_bool(state.status(node_id) == *status),
            Self::Semantic { condition_id, .. } => {
                input.semantic_results.get(condition_id).copied().unwrap_or(Truth::Unknown)
            }
        }
    }

    pub(super) fn holds(&self, state: &NarrativeState, input: &NarrativeEvidence<'_>) -> bool {
        self.evaluate(state, input) == Truth::Satisfied
    }

    pub(super) fn validate(&self, nodes: &BTreeMap<NarrativeNodeId, NarrativeNode>) -> Result<(), NarrativeError> {
        match self {
            Self::All { conditions } => validate_children("all", conditions, nodes),
            Self::Any { conditions } => validate_children("any", conditions, nodes),
            Self::Not { condition } => condition.validate(nodes),
            Self::NodeIs { node_id, .. } if !nodes.contains_key(node_id) => Err(NarrativeError::UnknownNode {
                node_id: node_id.to_string(),
            }),
            Self::NodeIs { .. } | Self::StoryStarted | Self::TurnReaches { .. } | Self::Semantic { .. } => Ok(()),
        }
    }

    pub(super) fn collect_semantic(
        &self,
        found: &mut BTreeMap<NarrativeConditionId, BoundedText>,
    ) -> Result<(), NarrativeError> {
        match self {
            Self::All { conditions } | Self::Any { conditions } => {
                conditions.iter().try_for_each(|condition| condition.collect_semantic(found))
            }
            Self::Not { condition } => condition.collect_semantic(found),
            Self::Semantic {
                condition_id,
                criterion,
            } => match found.get(condition_id) {
                Some(existing) if existing != criterion => Err(NarrativeError::SemanticCriterionConflict {
                    condition_id: condition_id.to_string(),
                }),
                Some(_) => Ok(()),
                None => {
                    found.insert(condition_id.clone(), criterion.clone());
                    Ok(())
                }
            },
            Self::StoryStarted | Self::TurnReaches { .. } | Self::NodeIs { .. } => Ok(()),
        }
    }
}

fn validate_children(
    operator: &'static str,
    conditions: &[NarrativeCondition],
    nodes: &BTreeMap<NarrativeNodeId, NarrativeNode>,
) -> Result<(), NarrativeError> {
    if conditions.is_empty() {
        return Err(NarrativeError::EmptyCondition { operator });
    }
    conditions.iter().try_for_each(|condition| condition.validate(nodes))
}

#[cfg(test)]
#[path = "tests/condition_tests.rs"]
mod tests;
