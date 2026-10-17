use crate::core::{CoreError, PackRef, StoryError, StoryId};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PersistenceError {
    #[error("persistance resource not found")]
    NotFound,
    #[error("create story instance failed. story_id: {story_id}, error message: {message}")]
    CreateStoryInstanceFailed { story_id: StoryId, message: String },
    #[error("persistance constraint violation: {constraint}")]
    ConstraintViolation { constraint: String },
    #[error("pack capacity {actual} exceeds maximum {maximum}")]
    PackCapacityExceeded { actual: usize, maximum: usize },
    #[error("story already holds a pack snapshot: {story_id}")]
    StoryHolderAlreadyExists { story_id: StoryId },
    #[error("invalid pack reference: {pack_ref:?}")]
    InvalidPackReference { pack_ref: PackRef },
    #[error("persistance atomicity failure: {operation}")]
    AtomicityFailure { operation: String },
    #[error("persistance unavailable: {message}")]
    Unavailable { message: String },
    #[error("persistance invalid data: {0}")]
    InvalidData(#[from] CoreError),
    #[error("persistance invalid story data: {0}")]
    InvalidStory(#[from] StoryError),
    #[error("story creation and pack cleanup failed for {story_id}: create={create_source}; cleanup={cleanup_source}")]
    CreateCompensationFailed {
        story_id: StoryId,
        create_source: Box<PersistenceError>,
        cleanup_source: Box<PersistenceError>,
    },
}
