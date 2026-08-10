use crate::persistence::store::StoreError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ContextError {
    #[error("story snapshot is inconsistent: {code}")]
    SnapshotInconsistent { code: &'static str },
    #[error("story continuity is invalid: {code}")]
    ContinuityInvalid { code: &'static str },
    #[error("retrieval signal limit exceeded: {limit}")]
    SignalLimitExceeded { limit: &'static str },
    #[error("retrieval plan is invalid: {code}")]
    InvalidPlan { code: &'static str },
    #[error("knowledge audience violation")]
    KnowledgeAudienceViolation,
    #[error("candidate retriever configuration is invalid: {code}")]
    InvalidRetrieverSet { code: &'static str },
    #[error("retrieval record is invalid: {code}")]
    InvalidRecord { code: &'static str },
    #[error("retrieval candidate limit exceeded")]
    CandidateLimitExceeded,
    #[error("retrieved context budget exceeded: {limit}")]
    RetrievedBudgetExceeded { limit: &'static str },
    #[error("knowledge read failed")]
    Store(#[from] StoreError),
}

impl ContextError {
    pub fn turn_code(&self) -> &'static str {
        match self {
            ContextError::SnapshotInconsistent { .. } | ContextError::ContinuityInvalid { .. } => {
                "context_snapshot_invalid"
            }
            ContextError::SignalLimitExceeded { .. } => "context_baseline_limit",
            ContextError::InvalidPlan { .. } | ContextError::KnowledgeAudienceViolation => "writer_plan_invalid",
            ContextError::InvalidRetrieverSet { .. } => "retrieval_candidate_limit",
            ContextError::InvalidRecord { .. } => "retrieval_record_invalid",
            ContextError::CandidateLimitExceeded => "retrieval_candidate_limit",
            ContextError::RetrievedBudgetExceeded { .. } => "retrieval_context_limit",
            ContextError::Store(StoreError::RevisionConflict) => "retrieval_snapshot_conflict",
            ContextError::Store(_) => "store_error",
        }
    }
}
