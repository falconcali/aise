use crate::core::{CommittedTurnInfo, ExecuteTurnSpec, TurnEvent, TurnEventSink, TurnResult};
use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("turn execution failed: {message}")]
    Turn { message: String },
    #[error("engine dependency failed: {message}")]
    Dependency { message: String },
    #[error("engine is not initialized")]
    NotInitialized,
}

#[async_trait]
pub trait Engine: Send + Sync {
    async fn run_turn(&self, spec: ExecuteTurnSpec, sink: &dyn TurnEventSink) -> Result<TurnResult, EngineError>;
}

#[derive(Default)]
pub struct HelloWorldEngine;

#[async_trait]
impl Engine for HelloWorldEngine {
    async fn run_turn(&self, _spec: ExecuteTurnSpec, sink: &dyn TurnEventSink) -> Result<TurnResult, EngineError> {
        sink.emit(TurnEvent::StageStarted {
            stage: "hello_world".into(),
        })
        .map_err(|error| EngineError::Turn {
            message: error.to_string(),
        })?;
        let result = TurnResult {
            result: CommittedTurnInfo {
                turn_number: 1,
                story_revision: 1,
                story_text: "hello world".into(),
            },
            replayed: false,
        };
        sink.emit(TurnEvent::Committed {
            result: result.result.clone(),
            replayed: result.replayed,
        })
        .map_err(|error| EngineError::Turn {
            message: error.to_string(),
        })?;
        Ok(result)
    }
}
