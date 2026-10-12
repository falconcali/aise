use crate::core::{AssetError, CoreError, StoryId};
use thiserror::Error;

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
    #[error("persistance invalid data: {0}")]
    InvalidData(#[from] CoreError),
    #[error("persistance invalid asset: {0}")]
    InvalidAsset(#[from] AssetError),
}
