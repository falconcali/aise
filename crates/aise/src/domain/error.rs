use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DomainInputError {
    #[error("story_id must not be empty")]
    EmptyStoryId,
    #[error("constraint_id must not be empty")]
    EmptyConstraintId,
    #[error("character_id must be a canonical UUID")]
    InvalidCharacterId,
    #[error("role_id must match [a-z0-9]+(?:[._-][a-z0-9]+)* and contain at most 128 bytes")]
    InvalidRoleId,
    #[error("role_id must not collide with a canonical knowledge id shape")]
    RoleIdReservedForKnowledge,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum KnowledgeIdError {
    #[error("knowledge id has invalid grammar: {value}")]
    InvalidGrammar { value: String },
    #[error("knowledge id sequence exceeds the sqlite signed integer range")]
    SequenceOverflow,
    #[error("knowledge id allocation would overflow the sequence space")]
    AllocationOverflow,
}
