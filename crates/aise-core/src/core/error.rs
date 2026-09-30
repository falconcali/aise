use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("invalid ID: {field}-{value}")]
    InvalidId { field: &'static str, value: String },
}

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("turn execution failed: {message}")]
    Turn { message: String },
    #[error("engine dependency failed: {message}")]
    Dependency { message: String },
    #[error("engine is not initialized")]
    NotInitialized,
}
