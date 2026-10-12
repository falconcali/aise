use crate::core::narrative::{NarrativeError, NarrativeGraph, NarrativeState, NodeStatus};
use crate::core::{BoundedText, NarrativeConditionId, NarrativeNodeId};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NarrativeDirection {
    pub node_id: NarrativeNodeId,
    pub dramatic_focus: BoundedText,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActiveConstraint {
    pub node_id: NarrativeNodeId,
    pub forbid: BoundedText,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SemanticQuery {
    pub condition_id: NarrativeConditionId,
    pub criterion: BoundedText,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct NarrativePlan {
    pub directions: Vec<NarrativeDirection>,
    pub constraints: Vec<ActiveConstraint>,
    pub queries: Vec<SemanticQuery>,
}

impl NarrativeGraph {
    pub fn plan(&self, state: &NarrativeState) -> Result<NarrativePlan, NarrativeError> {
        Ok(NarrativePlan {
            directions: self.directions(state),
            constraints: self.constraints(state),
            queries: self.queries(state)?,
        })
    }

    fn constraints(&self, state: &NarrativeState) -> Vec<ActiveConstraint> {
        self.nodes
            .iter()
            .flat_map(|(node_id, node)| {
                let status = state.status(node_id);
                node.constraints
                    .iter()
                    .filter(move |constraint| constraint.applies(status))
                    .map(|constraint| ActiveConstraint {
                        node_id: node_id.clone(),
                        forbid: constraint.forbid.clone(),
                    })
            })
            .collect()
    }

    fn directions(&self, state: &NarrativeState) -> Vec<NarrativeDirection> {
        self.nodes
            .iter()
            .filter(|(node_id, _)| state.status(node_id) == NodeStatus::Active)
            .filter_map(|(node_id, node)| {
                node.dramatic_focus.clone().map(|dramatic_focus| NarrativeDirection {
                    node_id: node_id.clone(),
                    dramatic_focus,
                })
            })
            .collect()
    }

    fn queries(&self, state: &NarrativeState) -> Result<Vec<SemanticQuery>, NarrativeError> {
        let mut found = BTreeMap::new();
        for (node_id, node) in &self.nodes {
            for condition in node.watched_conditions(state.status(node_id)) {
                condition.collect_semantic(&mut found)?;
            }
        }
        Ok(found
            .into_iter()
            .map(|(condition_id, criterion)| SemanticQuery {
                condition_id,
                criterion,
            })
            .collect())
    }
}

#[cfg(test)]
#[path = "tests/plan_tests.rs"]
mod tests;
