use crate::domain::ids::TurnId;
use crate::turn::turn_contract::CommittedTurnResult;
use crate::turn::turn_pipeline::TurnStage;
use crate::turn::turn_trace::TurnTrace;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Clone)]
pub enum TurnEvent {
    StageStarted {
        turn_id: TurnId,
        stage: TurnStage,
    },

    ValidationCompleted {
        turn_id: TurnId,
        attempt: u32,
        decision: crate::turn::turn_validation::ValidationDecision,
        issue_codes: Vec<crate::turn::turn_validation::ValidationIssueCode>,
    },

    Committed {
        result: CommittedTurnResult,
        replayed: bool,
    },

    Failed {
        turn_id: TurnId,
        code: &'static str,
    },

    Cancelled {
        turn_id: TurnId,
        code: &'static str,
    },

    Conflict {
        turn_id: TurnId,
        code: &'static str,
    },

    TraceCompleted {
        turn_id: TurnId,
        trace: TurnTrace,
    },
}

impl TurnEvent {
    pub fn turn_id(&self) -> Option<&TurnId> {
        match self {
            TurnEvent::StageStarted { turn_id, .. } => Some(turn_id),
            TurnEvent::ValidationCompleted { turn_id, .. } => Some(turn_id),
            TurnEvent::Committed { .. } => None,
            TurnEvent::Failed { turn_id, .. } => Some(turn_id),
            TurnEvent::Cancelled { turn_id, .. } => Some(turn_id),
            TurnEvent::Conflict { turn_id, .. } => Some(turn_id),
            TurnEvent::TraceCompleted { turn_id, .. } => Some(turn_id),
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
            TurnEvent::StageStarted { turn_id, stage } => {
                json!({ "turn_id": turn_id.as_str(), "stage": stage.as_str() })
            }
            TurnEvent::ValidationCompleted {
                turn_id,
                attempt,
                decision,
                issue_codes,
            } => json!({
                "turn_id": turn_id.as_str(),
                "attempt": attempt,
                "decision": decision.as_str(),
                "issue_codes": issue_codes.iter().map(|code| code.as_str()).collect::<Vec<_>>(),
            }),
            TurnEvent::Committed { result, replayed } => json!({
                "turn_id": result.turn_id.as_str(),
                "story_revision": result.story_revision.get(),
                "replayed": replayed,
            }),
            TurnEvent::Failed { turn_id, code } => json!({ "turn_id": turn_id.as_str(), "code": code }),
            TurnEvent::Cancelled { turn_id, code } => json!({ "turn_id": turn_id.as_str(), "code": code }),
            TurnEvent::Conflict { turn_id, code } => json!({ "turn_id": turn_id.as_str(), "code": code }),
            TurnEvent::TraceCompleted { trace, .. } => serde_json::to_value(trace).unwrap_or(serde_json::Value::Null),
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
