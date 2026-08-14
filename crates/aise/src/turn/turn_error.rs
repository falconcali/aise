use crate::turn::turn_pipeline::TurnStage;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnFailureKind {
    InvalidRequest,
    StoryNotFound,
    Cancelled,
    DeadlineExceeded,
    RevisionConflict,
    IdempotencyConflict,
    Backpressure,
    ValidationRejected,
    ValidationBudgetExhausted,
    TokenBudgetExceeded,
    Llm,
    Store,
    Io,
    InvariantViolation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnTerminalKind {
    Failed,
    Cancelled,
    Conflict,
}

#[derive(Debug, Error)]
pub struct TurnExecutionError {
    kind: TurnFailureKind,
    code: &'static str,
    stage: Option<TurnStage>,
    message: String,
}

impl Clone for TurnExecutionError {
    fn clone(&self) -> Self {
        Self {
            kind: self.kind,
            code: self.code,
            stage: self.stage,
            message: self.message.clone(),
        }
    }
}

impl TurnExecutionError {
    pub fn new(
        kind: TurnFailureKind,
        code: &'static str,
        stage: Option<TurnStage>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            code,
            stage,
            message: message.into(),
        }
    }

    pub fn terminal_kind(&self) -> TurnTerminalKind {
        match self.kind {
            TurnFailureKind::Cancelled => TurnTerminalKind::Cancelled,
            TurnFailureKind::RevisionConflict | TurnFailureKind::IdempotencyConflict => TurnTerminalKind::Conflict,
            _ => TurnTerminalKind::Failed,
        }
    }

    pub fn kind(&self) -> TurnFailureKind {
        self.kind
    }

    pub fn code(&self) -> &'static str {
        self.code
    }

    pub fn stage(&self) -> Option<TurnStage> {
        self.stage
    }

    pub fn cancelled(stage: Option<TurnStage>) -> Self {
        Self::new(TurnFailureKind::Cancelled, "cancelled", stage, "turn cancelled")
    }

    pub fn deadline_exceeded(stage: Option<TurnStage>) -> Self {
        Self::new(
            TurnFailureKind::DeadlineExceeded,
            "deadline_exceeded",
            stage,
            "turn deadline exceeded",
        )
    }

    pub fn revision_conflict(stage: Option<TurnStage>) -> Self {
        Self::new(
            TurnFailureKind::RevisionConflict,
            "revision_conflict",
            stage,
            "revision conflict",
        )
    }

    pub fn idempotency_conflict(stage: Option<TurnStage>) -> Self {
        Self::new(
            TurnFailureKind::IdempotencyConflict,
            "idempotency_conflict",
            stage,
            "idempotency conflict",
        )
    }

    pub fn validation_budget_exhausted(rounds: u32) -> Self {
        Self::new(
            TurnFailureKind::ValidationBudgetExhausted,
            "validation_budget_exhausted",
            Some(TurnStage::Validation),
            format!("validation failed after {rounds} repair rounds; giving up"),
        )
    }

    pub fn validation_rejected(detail: String) -> Self {
        Self::new(
            TurnFailureKind::ValidationRejected,
            "validation_rejected",
            Some(TurnStage::Validation),
            detail,
        )
    }

    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(TurnFailureKind::InvalidRequest, "invalid_request", None, message.into())
    }

    pub fn stale_state_extraction(stage: Option<TurnStage>) -> Self {
        Self::new(
            TurnFailureKind::InvariantViolation,
            "stale_state_extraction",
            stage,
            "state extraction is bound to a story version older than the current candidate story",
        )
    }

    pub fn story_repair_no_progress(stage: Option<TurnStage>) -> Self {
        Self::new(
            TurnFailureKind::ValidationRejected,
            "story_repair_no_progress",
            stage,
            "story repair produced byte-identical prose with unresolved issues",
        )
    }

    pub fn state_reextraction_no_progress(stage: Option<TurnStage>) -> Self {
        Self::new(
            TurnFailureKind::ValidationRejected,
            "state_reextraction_no_progress",
            stage,
            "state re-extraction produced byte-identical output with unresolved issues",
        )
    }

    pub fn state_extractor_required_context_exceeds_budget(
        stage: Option<TurnStage>,
        message: impl Into<String>,
    ) -> Self {
        Self::new(
            TurnFailureKind::TokenBudgetExceeded,
            "state_extractor_required_context_exceeds_budget",
            stage,
            message.into(),
        )
    }

    pub fn story_not_found(story_id: &str) -> Self {
        Self::new(
            TurnFailureKind::StoryNotFound,
            "story_not_found",
            None,
            format!("story {story_id} not found"),
        )
    }

    pub fn invariant(message: impl Into<String>) -> Self {
        Self::new(TurnFailureKind::InvariantViolation, "invariant_violation", None, message.into())
    }

    pub fn backpressure(message: impl Into<String>) -> Self {
        Self::new(TurnFailureKind::Backpressure, "backpressure", None, message.into())
    }
}

impl std::fmt::Display for TurnExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code)
    }
}
