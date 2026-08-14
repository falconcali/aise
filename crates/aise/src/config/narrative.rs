use super::error::ConfigError;
use crate::domain::narrative_graph::definition::NarrativeLimits;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NarrativeConfig {
    pub max_graph_nodes: usize,
    pub max_graph_edges: usize,
    pub max_condition_depth: usize,
    pub max_conditions_per_node: usize,
    pub max_effects_per_node: usize,
    pub max_semantic_conditions: usize,
    pub max_semantic_criterion_bytes: usize,
    pub max_frontier_nodes: usize,
    pub max_semantic_queries_per_turn: usize,
    pub max_semantic_query_bytes: usize,
    pub max_evidence_bytes: usize,
    pub max_result_reason_bytes: usize,
    pub max_transitions_per_turn: usize,
    pub max_pending_effects: usize,
}

impl Default for NarrativeConfig {
    fn default() -> Self {
        Self {
            max_graph_nodes: 256,
            max_graph_edges: 512,
            max_condition_depth: 8,
            max_conditions_per_node: 16,
            max_effects_per_node: 16,
            max_semantic_conditions: 64,
            max_semantic_criterion_bytes: 2 * 1024,
            max_frontier_nodes: 64,
            max_semantic_queries_per_turn: 32,
            max_semantic_query_bytes: 16 * 1024,
            max_evidence_bytes: 1024,
            max_result_reason_bytes: 1024,
            max_transitions_per_turn: 32,
            max_pending_effects: 128,
        }
    }
}

impl NarrativeConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.max_graph_nodes == 0 {
            return Err(ConfigError::Invalid("narrative.max_graph_nodes must be positive".into()));
        }
        if self.max_graph_edges == 0 {
            return Err(ConfigError::Invalid("narrative.max_graph_edges must be positive".into()));
        }
        if self.max_condition_depth == 0 {
            return Err(ConfigError::Invalid("narrative.max_condition_depth must be positive".into()));
        }
        if self.max_conditions_per_node == 0 {
            return Err(ConfigError::Invalid(
                "narrative.max_conditions_per_node must be positive".into(),
            ));
        }
        if self.max_effects_per_node == 0 {
            return Err(ConfigError::Invalid("narrative.max_effects_per_node must be positive".into()));
        }
        if self.max_semantic_conditions == 0 {
            return Err(ConfigError::Invalid(
                "narrative.max_semantic_conditions must be positive".into(),
            ));
        }
        if self.max_semantic_criterion_bytes == 0 {
            return Err(ConfigError::Invalid(
                "narrative.max_semantic_criterion_bytes must be positive".into(),
            ));
        }
        if self.max_frontier_nodes == 0 {
            return Err(ConfigError::Invalid("narrative.max_frontier_nodes must be positive".into()));
        }
        if self.max_semantic_queries_per_turn == 0 {
            return Err(ConfigError::Invalid(
                "narrative.max_semantic_queries_per_turn must be positive".into(),
            ));
        }
        if self.max_semantic_query_bytes == 0 {
            return Err(ConfigError::Invalid(
                "narrative.max_semantic_query_bytes must be positive".into(),
            ));
        }
        if self.max_evidence_bytes == 0 {
            return Err(ConfigError::Invalid("narrative.max_evidence_bytes must be positive".into()));
        }
        if self.max_result_reason_bytes == 0 {
            return Err(ConfigError::Invalid(
                "narrative.max_result_reason_bytes must be positive".into(),
            ));
        }
        if self.max_transitions_per_turn == 0 {
            return Err(ConfigError::Invalid(
                "narrative.max_transitions_per_turn must be positive".into(),
            ));
        }
        if self.max_pending_effects == 0 {
            return Err(ConfigError::Invalid("narrative.max_pending_effects must be positive".into()));
        }
        Ok(())
    }

    pub fn as_limits(&self) -> NarrativeLimits {
        NarrativeLimits {
            max_graph_nodes: self.max_graph_nodes,
            max_graph_edges: self.max_graph_edges,
            max_condition_depth: self.max_condition_depth,
            max_conditions_per_node: self.max_conditions_per_node,
            max_effects_per_node: self.max_effects_per_node,
            max_semantic_conditions: self.max_semantic_conditions,
            max_semantic_criterion_bytes: self.max_semantic_criterion_bytes,
            max_frontier_nodes: self.max_frontier_nodes,
            max_semantic_queries_per_turn: self.max_semantic_queries_per_turn,
            max_semantic_query_bytes: self.max_semantic_query_bytes,
            max_evidence_bytes: self.max_evidence_bytes,
            max_result_reason_bytes: self.max_result_reason_bytes,
            max_transitions_per_turn: self.max_transitions_per_turn,
            max_pending_effects: self.max_pending_effects,
        }
    }
}
