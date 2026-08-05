use crate::core::turn_contract::CommittedTurnResult;
use crate::core::turn_pipeline::TurnStage;
use crate::core::turn_trace::TurnTrace;
use crate::domain::ids::TurnId;

#[derive(Debug, Clone)]
pub enum TurnEvent {
    StageStarted(TurnStage),

    ValidationCompleted { pass: bool },

    Committed(CommittedTurnResult),

    Failed { turn_id: TurnId, error: String },

    Cancelled { turn_id: TurnId },

    Conflict { turn_id: TurnId },

    TraceCompleted(TurnTrace),
}

pub trait TurnEventSink: Send + Sync {
    fn emit(&self, event: TurnEvent);
}
