use crate::core::{CommittedTurnInfo, EngineError, TurnControl, TurnEvent, TurnEventSink, TurnRequest, TurnResult};
use crate::llm::LlmGateway;
use crate::pipeline::Runtime;
use crate::prompt::Prompt;
use crate::trace::Trace;
use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait Engine: Send + Sync {
    async fn run_turn(
        &self,
        turn_request: TurnRequest,
        turn_control: TurnControl,
        sink: &dyn TurnEventSink,
        trace: &Trace,
    ) -> Result<TurnResult, EngineError>;
}

pub struct AiseEngine {
    runtime: Runtime,
}

impl AiseEngine {
    pub fn new(gateway: Arc<LlmGateway>, prompt: Arc<Prompt>) -> Self {
        Self {
            runtime: Runtime::new(gateway, prompt),
        }
    }
}

#[async_trait]
impl Engine for AiseEngine {
    async fn run_turn(
        &self,
        turn_request: TurnRequest,
        turn_control: TurnControl,
        sink: &dyn TurnEventSink,
        trace: &Trace,
    ) -> Result<TurnResult, EngineError> {
        sink.emit(TurnEvent::StageStarted {
            stage: "initialization".to_string(),
        })
        .map_err(|error| EngineError::Turn {
            message: error.to_string(),
        })?;

        let turn_result = self
            .runtime
            .run_turn(turn_request, turn_control, sink, trace)
            .await
            .map_err(|error| EngineError::Turn {
                message: error.to_string(),
            })?;

        sink.emit(TurnEvent::Committed {
            result: turn_result.result.clone(),
            replayed: turn_result.replayed,
        })
        .map_err(|error| EngineError::Turn {
            message: error.to_string(),
        })?;

        Ok(turn_result)
    }
}
