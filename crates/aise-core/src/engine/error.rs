use crate::persistence::PersistenceError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("turn execution failed: {message}")]
    Turn { message: String },
    #[error("engine dependency failed: {message}")]
    Dependency { message: String },
    #[error("persistence error: {message}")]
    Persistence { message: String },
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
