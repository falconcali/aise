use crate::domain::narrative_graph::definition::NarrativeError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PlanningError {
    #[error("narrative evaluation failed")]
    Narrative(#[from] NarrativeError),
    #[error("writer planner output is invalid: {code}")]
    InvalidOutput { code: &'static str },
    #[error("writer planner referenced an unknown role")]
    UnknownRole,
    #[error("writer planner requested a player-controlled role")]
    PlayerRoleRequested,
    #[error("writer planner requested a role more than once")]
    DuplicateRoleTarget,
    #[error("writer planner referenced an unknown entity or topic")]
    UnknownRetrievalKey,
    #[error("writer planner violated knowledge audience rules")]
    KnowledgeAudienceViolation,
    #[error("writer plan limit exceeded: {limit}")]
    LimitExceeded { limit: &'static str },
}

impl PlanningError {
    pub fn turn_code(&self) -> &'static str {
        match self {
            PlanningError::Narrative(_) => "narrative_evaluation_failed",
            _ => "writer_plan_invalid",
        }
    }
}
