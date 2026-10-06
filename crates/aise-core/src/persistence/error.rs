use thiserror::Error;
use crate::core::ids::StoryId;

#[derive(Error, Debug)]
pub enum PersistenceError {
    #[error("persistance resource not found")]
    NotFound,
    #[error("create story instance failed. story_id: {story_id}, error message: {message}")]
    CreateStoryInstanceFailed { story_id: StoryId, message: String },
    #[error("persistance constraint violation: {constraint}")]
    ConstraintViolation { constraint: String },
    #[error("persistance unavailable: {message}")]
    Unavailable { message: String },
}