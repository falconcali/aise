use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DomainInputError {
    #[error("story_id must not be empty")]
    EmptyStoryId,
    #[error("turn_id must not be empty")]
    EmptyTurnId,
    #[error("constraint_id must not be empty")]
    EmptyConstraintId,
}
