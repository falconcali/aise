use crate::core::{AssetError, StoryLifeCycle};
use crate::llm::LlmError;
use crate::persistence::PersistenceError;
use crate::prompt::PromptError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("turn execution failed: {message}")]
    Turn { message: String },
    #[error("story is not active: {life_cycle:?}")]
    StoryNotActive { life_cycle: StoryLifeCycle },
    #[error("engine dependency failed: {message}")]
    Dependency { message: String },
    #[error("persistence error: {message}")]
    Persistence { message: String },
    #[error("asset error: {message}")]
    Asset { message: String },
    #[error("prompt error: {message}")]
    Prompt { message: String },
    #[error("llm error: {message}")]
    Llm { message: String },
    #[error("engine is not initialized")]
    NotInitialized,
}

impl From<PersistenceError> for EngineError {
    fn from(error: PersistenceError) -> Self {
        EngineError::Persistence {
            message: error.to_string(),
        }
    }
}

impl From<AssetError> for EngineError {
    fn from(error: AssetError) -> Self {
        EngineError::Asset {
            message: error.to_string(),
        }
    }
}

impl From<PromptError> for EngineError {
    fn from(error: PromptError) -> Self {
        EngineError::Prompt {
            message: error.to_string(),
        }
    }
}

impl From<LlmError> for EngineError {
    fn from(error: LlmError) -> Self {
        EngineError::Llm {
            message: error.to_string(),
        }
    }
}
