use crate::domain::asset::ids::{NarrativeEdgeKey, NarrativeNodeKey};
use crate::domain::asset::validation::BoundedText;
use crate::domain::narrative_graph::condition::NarrativeCondition;
use crate::domain::narrative_graph::effect::NarrativeEffectDefinition;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy)]
pub struct NarrativeLimits {
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

#[derive(Debug, Clone, thiserror::Error)]
pub enum NarrativeError {
    #[error("narrative reference is missing: {key}")]
    MissingReference { key: String },
    #[error("narrative condition depth limit exceeded")]
    ConditionDepthExceeded,
    #[error("narrative condition count limit exceeded")]
    ConditionCountExceeded,
    #[error("narrative graph node limit exceeded")]
    GraphNodeLimitExceeded,
    #[error("narrative graph edge limit exceeded")]
    GraphEdgeLimitExceeded,
    #[error("narrative semantic condition key {key} is reused with a different criterion")]
    SemanticCriterionConflict { key: String },
    #[error("narrative semantic condition count limit exceeded")]
    SemanticConditionLimitExceeded,
    #[error("narrative frontier node limit exceeded")]
    FrontierLimitExceeded,
    #[error("narrative semantic query count limit exceeded")]
    SemanticQueryLimitExceeded,
    #[error("narrative semantic query byte limit exceeded")]
    SemanticQueryByteLimitExceeded,
    #[error("narrative transition limit exceeded")]
    TransitionLimitExceeded,
    #[error("narrative pending effect limit exceeded")]
    PendingEffectLimitExceeded,
    #[error("narrative candidate version mismatch")]
    CandidateVersionMismatch,
    #[error("narrative graph revision mismatch")]
    GraphRevisionMismatch,
    #[error("narrative invariant violated: {code}")]
    Invariant { code: &'static str },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NarrativeGraphDefinition {
    pub entry_nodes: Vec<NarrativeNodeKey>,
    pub nodes: BTreeMap<NarrativeNodeKey, NarrativeNodeDefinition>,
    pub edges: Vec<NarrativeEdgeDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NarrativeNodeDefinition {
    pub title: BoundedText,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dramatic_focus: Option<BoundedText>,
    pub activate_when: NarrativeCondition,
    pub complete_when: NarrativeCondition,
    pub skip_when: Option<NarrativeCondition>,
    pub effects: NarrativeNodeEffects,
    pub terminal: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NarrativeNodeEffects {
    #[serde(default)]
    pub on_activate: Vec<NarrativeEffectDefinition>,
    #[serde(default)]
    pub on_complete: Vec<NarrativeEffectDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NarrativeEdgeDefinition {
    pub edge_key: NarrativeEdgeKey,
    pub from: NarrativeNodeKey,
    pub to: NarrativeNodeKey,
    pub when: NarrativeCondition,
}
