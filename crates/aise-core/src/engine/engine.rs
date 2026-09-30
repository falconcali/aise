use crate::core::{CommittedTurnInfo, EngineError, TurnControl, TurnEvent, TurnEventSink, TurnRequest, TurnResult};
use crate::pipeline::Runtime;
use crate::trace::Trace;
use async_trait::async_trait;

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

#[derive(Default)]
pub struct AiseEngine {
    runtime: Runtime,
}

impl AiseEngine {
    pub fn new() -> Self {
        Self {
            runtime: Runtime::new(),
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

        let turn_result = self.runtime.run_turn(turn_request, turn_control, sink, trace).await?;

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
