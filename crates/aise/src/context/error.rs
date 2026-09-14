use crate::domain::knowledge::activation::{ActivationError, ActivationStoreFailure};
use crate::persistence::store::StoreError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ContextError {
    #[error("story snapshot is inconsistent: {code}")]
    SnapshotInconsistent { code: &'static str },
    #[error("story continuity is invalid: {code}")]
    ContinuityInvalid { code: &'static str },
    #[error("retrieval plan is invalid: {code}")]
    InvalidPlan { code: &'static str },
    #[error("knowledge audience violation")]
    KnowledgeAudienceViolation,
    #[error("retrieval record is invalid: {code}")]
    InvalidRecord { code: &'static str },
    #[error("retrieval candidate limit exceeded")]
    CandidateLimitExceeded,
    #[error("retrieved context budget exceeded: {limit}")]
    RetrievedBudgetExceeded { limit: &'static str },
    #[error("index limit exceeded: {index} actual {actual} maximum {maximum}")]
    IndexLimitExceeded {
        index: &'static str,
        actual: usize,
        maximum: usize,
    },
    #[error("knowledge activation failed")]
    Activation(#[from] ActivationError),
    #[error("knowledge read failed")]
    Store(#[from] StoreError),
}

impl ContextError {
    pub fn turn_code(&self) -> &'static str {
        match self {
            ContextError::SnapshotInconsistent { .. } | ContextError::ContinuityInvalid { .. } => {
                "context_snapshot_invalid"
            }
            ContextError::InvalidPlan { .. } | ContextError::KnowledgeAudienceViolation => "writer_plan_invalid",
            ContextError::InvalidRecord { .. } => "retrieval_record_invalid",
            ContextError::CandidateLimitExceeded => "retrieval_candidate_limit",
            ContextError::RetrievedBudgetExceeded { .. } => "retrieval_context_limit",
            ContextError::IndexLimitExceeded { .. } => "context_index_limit_exceeded",
            ContextError::Activation(ActivationError::Store(ActivationStoreFailure::RevisionConflict)) => {
                "retrieval_snapshot_conflict"
            }
            ContextError::Activation(ActivationError::Store(_)) => "store_error",
            ContextError::Activation(error) => error.code(),
            ContextError::Store(StoreError::RevisionConflict) => "retrieval_snapshot_conflict",
            ContextError::Store(_) => "store_error",
        }
    }
}
