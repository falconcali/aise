use crate::core::turn_pipeline::TurnStage;
use crate::core::turn_trace::TurnTrace;
use crate::domain::ids::TurnId;

#[derive(Debug, Clone)]
pub enum TurnEvent {
    StageStarted(TurnStage),

    Token(String),

    Validation { pass: bool },

    Finished { turn_id: TurnId },

    Trace(TurnTrace),
}

pub trait TurnEventSink: Send + Sync {
    fn emit(&self, event: TurnEvent);
}
