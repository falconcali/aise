use thiserror::Error;

#[derive(Debug, Error)]
pub enum NarrativeError {
    #[error("narrative {operator} condition must not be empty")]
    EmptyCondition { operator: &'static str },

    #[error("narrative condition references unknown node: {node_id}")]
    UnknownNode { node_id: String },

    #[error("semantic condition {condition_id} is declared with different criteria")]
    SemanticCriterionConflict { condition_id: String },
}
