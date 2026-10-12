use thiserror::Error;

#[derive(Debug, Error)]
pub enum NarrativeError {
    #[error("narrative graph has too many nodes: {actual} (maximum: {maximum})")]
    TooManyNodes { actual: usize, maximum: usize },

    #[error("narrative condition nesting exceeds maximum depth: {maximum}")]
    ConditionTooDeep { maximum: usize },

    #[error("narrative {operator} condition must not be empty")]
    EmptyCondition { operator: &'static str },

    #[error("narrative condition references unknown node: {node_id}")]
    UnknownNode { node_id: String },

    #[error("semantic condition {condition_id} is declared with different criteria")]
    SemanticCriterionConflict { condition_id: String },

    #[error("narrative node {node_id} has too many constraints: {actual} (maximum: {maximum})")]
    TooManyConstraints {
        node_id: String,
        actual: usize,
        maximum: usize,
    },

    #[error("narrative graph has too many semantic conditions: {actual} (maximum: {maximum})")]
    TooManySemanticConditions { actual: usize, maximum: usize },
}
