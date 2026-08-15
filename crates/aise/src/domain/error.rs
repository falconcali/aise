use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DomainInputError {
    #[error("story_id must not be empty")]
    EmptyStoryId,
    #[error("turn_id must not be empty")]
    EmptyTurnId,
    #[error("constraint_id must not be empty")]
    EmptyConstraintId,
    #[error("character_id must be a canonical UUID")]
    InvalidCharacterId,
    #[error("role_id must match [a-z0-9]+(?:[._-][a-z0-9]+)* and contain at most 128 bytes")]
    InvalidRoleId,
}
