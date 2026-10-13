use crate::core::{CoreError, NarrativeError, StoryError, StoryLifeCycle};
use crate::llm::LlmError;
use crate::persistence::PersistenceError;
use crate::pipeline::PipelineError;
use crate::prompt::PromptError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("story is not active: {life_cycle:?}")]
    StoryNotActive { life_cycle: StoryLifeCycle },
    #[error("engine dependency failed: {message}")]
    Dependency { message: String },
    #[error("core error: {0}")]
    Core(#[from] CoreError),
    #[error("story error: {0}")]
    Story(#[from] StoryError),
    #[error("narrative error: {0}")]
    Narrative(#[from] NarrativeError),
    #[error("persistence error: {0}")]
    Persistence(#[from] PersistenceError),
    #[error("prompt error: {0}")]
    Prompt(#[from] PromptError),
    #[error("llm error: {0}")]
    Llm(#[from] LlmError),
    #[error("pipeline error: {0}")]
    Pipeline(#[from] PipelineError),
    #[error("engine is not initialized")]
    NotInitialized,
}
