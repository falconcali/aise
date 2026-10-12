use crate::core::narrative::{
    NarrativeCondition, NarrativeConfig, NarrativeConstraint, NarrativeError, NarrativeInput, NarrativeState,
    NodeStatus,
};
use crate::core::{BoundedText, NarrativeNodeId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NarrativeGraph {
    pub nodes: BTreeMap<NarrativeNodeId, NarrativeNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NarrativeNode {
    pub title: BoundedText,
    #[serde(default)]
    pub dramatic_focus: Option<BoundedText>,
    pub activate_when: NarrativeCondition,
    pub complete_when: NarrativeCondition,
    #[serde(default)]
    pub skip_when: Option<NarrativeCondition>,
    #[serde(default)]
    pub constraints: Vec<NarrativeConstraint>,
    #[serde(default)]
    pub terminal: bool,
}

impl NarrativeGraph {
    pub fn validate(&self, config: &NarrativeConfig) -> Result<(), NarrativeError> {
        if self.nodes.len() > config.max_nodes {
            return Err(NarrativeError::TooManyNodes {
                actual: self.nodes.len(),
                maximum: config.max_nodes,
            });
        }

        for (node_id, node) in &self.nodes {
            if node.constraints.len() > config.max_constraints_per_node {
                return Err(NarrativeError::TooManyConstraints {
                    node_id: node_id.to_string(),
                    actual: node.constraints.len(),
                    maximum: config.max_constraints_per_node,
                });
            }
        }

        let mut semantic = BTreeMap::new();
        for condition in self.nodes.values().flat_map(NarrativeNode::conditions) {
            condition.validate(&self.nodes, config.max_condition_depth)?;
            condition.collect_semantic(&mut semantic)?;
        }

        if semantic.len() > config.max_semantic_conditions {
            return Err(NarrativeError::TooManySemanticConditions {
                actual: semantic.len(),
                maximum: config.max_semantic_conditions,
            });
        }
        Ok(())
    }
}

impl NarrativeNode {
    pub(super) fn conditions(&self) -> impl Iterator<Item = &NarrativeCondition> {
        [
            Some(&self.activate_when),
            Some(&self.complete_when),
            self.skip_when.as_ref(),
        ]
        .into_iter()
        .flatten()
    }

    pub(super) fn watched_conditions(&self, status: NodeStatus) -> Vec<&NarrativeCondition> {
        match status {
            NodeStatus::Inactive => vec![&self.activate_when],
            NodeStatus::Active => self.skip_when.iter().chain([&self.complete_when]).collect(),
            NodeStatus::Completed | NodeStatus::Skipped => Vec::new(),
        }
    }

    pub(super) fn next_status(
        &self,
        current: NodeStatus,
        state: &NarrativeState,
        input: &NarrativeInput<'_>,
    ) -> Option<NodeStatus> {
        match current {
            NodeStatus::Inactive => self.activate_when.holds(state, input).then_some(NodeStatus::Active),
            NodeStatus::Active => {
                if self.skip_when.as_ref().is_some_and(|skip| skip.holds(state, input)) {
                    Some(NodeStatus::Skipped)
                } else {
                    self.complete_when.holds(state, input).then_some(NodeStatus::Completed)
                }
            }
            NodeStatus::Completed | NodeStatus::Skipped => None,
        }
    }
}

#[cfg(test)]
#[path = "tests/narrative_tests.rs"]
mod tests;
