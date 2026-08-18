use crate::domain::ids::TurnNumber;
use crate::turn::turn_contract::CommittedTurnResult;
use crate::turn::turn_pipeline::TurnStage;
use crate::turn::turn_trace::TurnTrace;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Clone)]
pub enum TurnEvent {
    StageStarted {
        turn_number: Option<TurnNumber>,
        stage: TurnStage,
    },

    ValidationCompleted {
        turn_number: Option<TurnNumber>,
        attempt: u32,
        decision: crate::turn::turn_validation::ValidationDecision,
        issue_codes: Vec<crate::turn::turn_validation::ValidationIssueCode>,
    },

    Committed {
        result: CommittedTurnResult,
        replayed: bool,
    },

    Failed {
        turn_number: Option<TurnNumber>,
        code: &'static str,
    },

    Cancelled {
        turn_number: Option<TurnNumber>,
        code: &'static str,
    },

    Conflict {
        turn_number: Option<TurnNumber>,
        code: &'static str,
    },

    TraceCompleted {
        trace: TurnTrace,
    },
}

impl TurnEvent {
    pub fn turn_number(&self) -> Option<TurnNumber> {
        match self {
            TurnEvent::StageStarted { turn_number, .. } => *turn_number,
            TurnEvent::ValidationCompleted { turn_number, .. } => *turn_number,
            TurnEvent::Committed { result, .. } => Some(result.turn_number),
            TurnEvent::Failed { turn_number, .. } => *turn_number,
            TurnEvent::Cancelled { turn_number, .. } => *turn_number,
            TurnEvent::Conflict { turn_number, .. } => *turn_number,
            TurnEvent::TraceCompleted { trace } => trace.turn_number,
        }
    }

    pub fn stage(&self) -> Option<TurnStage> {
        match self {
            TurnEvent::StageStarted { stage, .. } => Some(*stage),
            TurnEvent::ValidationCompleted { .. } => Some(TurnStage::Validation),
            TurnEvent::Committed { .. } => Some(TurnStage::TurnCommitter),
            _ => None,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TurnEvent::Committed { .. }
                | TurnEvent::Failed { .. }
                | TurnEvent::Cancelled { .. }
                | TurnEvent::Conflict { .. }
        )
    }
}

impl Serialize for TurnEvent {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("TurnEvent", 2)?;
        state.serialize_field("type", self.event_type())?;
        state.serialize_field("payload", &self.payload())?;
        state.end()
    }
}

impl TurnEvent {
    fn event_type(&self) -> &'static str {
        match self {
            TurnEvent::StageStarted { .. } => "stage_started",
            TurnEvent::ValidationCompleted { .. } => "validation_completed",
            TurnEvent::Committed { .. } => "committed",
            TurnEvent::Failed { .. } => "failed",
            TurnEvent::Cancelled { .. } => "cancelled",
            TurnEvent::Conflict { .. } => "conflict",
            TurnEvent::TraceCompleted { .. } => "trace_completed",
        }
    }

    fn payload(&self) -> serde_json::Value {
        use serde_json::json;
        match self {
            TurnEvent::StageStarted { turn_number, stage } => {
                json!({ "turn_number": turn_number.map(TurnNumber::get), "stage": stage.as_str() })
            }
            TurnEvent::ValidationCompleted {
                turn_number,
                attempt,
                decision,
                issue_codes,
            } => json!({
                "turn_number": turn_number.map(TurnNumber::get),
                "attempt": attempt,
                "decision": decision.as_str(),
                "issue_codes": issue_codes.iter().map(|code| code.as_str()).collect::<Vec<_>>(),
            }),
            TurnEvent::Committed { result, replayed } => json!({
                "turn_number": result.turn_number.get(),
                "story_revision": result.story_revision.get(),
                "replayed": replayed,
            }),
            TurnEvent::Failed { turn_number, code } => {
                json!({ "turn_number": turn_number.map(TurnNumber::get), "code": code })
            }
            TurnEvent::Cancelled { turn_number, code } => {
                json!({ "turn_number": turn_number.map(TurnNumber::get), "code": code })
            }
            TurnEvent::Conflict { turn_number, code } => {
                json!({ "turn_number": turn_number.map(TurnNumber::get), "code": code })
            }
            TurnEvent::TraceCompleted { trace } => serde_json::to_value(trace).unwrap_or(serde_json::Value::Null),
        }
    }
}

#[derive(Debug, Error)]
pub enum TurnEventDeliveryError {
    #[error("progress event channel backpressure")]
    ProgressBackpressure,
    #[error("terminal event already delivered")]
    TerminalAlreadySent,
    #[error("client disconnected")]
    ClientDisconnected,
}

pub trait TurnEventSink: Send + Sync {
    fn emit(&self, event: TurnEvent) -> Result<(), TurnEventDeliveryError>;
}
