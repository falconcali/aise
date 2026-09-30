use super::{IdempotencyKey, StoryId};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerContribution {
    pub raw: String,
    pub processed: String,
}

#[derive(Debug, Clone)]
pub struct TurnRequest {
    pub story_id: StoryId,
    pub idempotency_key: IdempotencyKey,
    pub player_input: String,
}

#[derive(Debug, Clone)]
pub struct TurnControl {
    deadline: Instant,
    cancellation: TurnCancellation,
}

impl TurnControl {
    pub fn new(deadline: Instant, cancellation: TurnCancellation) -> Self {
        Self { deadline, cancellation }
    }

    pub fn deadline(&self) -> Instant {
        self.deadline
    }

    pub fn cancellation(&self) -> &TurnCancellation {
        &self.cancellation
    }
}

#[derive(Debug, Clone)]
pub struct TurnCancellation(CancellationToken);

impl TurnCancellation {
    pub fn new() -> Self {
        Self(CancellationToken::new())
    }

    pub fn cancel(&self) {
        self.0.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }
}

impl Default for TurnCancellation {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommittedTurnInfo {
    pub turn_number: u64,
    pub story_revision: u64,
    pub story_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnResult {
    pub result: CommittedTurnInfo,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TurnEvent {
    StageStarted { stage: String },
    Committed { result: CommittedTurnInfo, replayed: bool },
    Failed { code: String },
    Cancelled { code: String },
    Conflict { code: String },
}

impl TurnEvent {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Committed { .. } | Self::Failed { .. } | Self::Cancelled { .. } | Self::Conflict { .. }
        )
    }
}

#[derive(Debug, Error)]
pub enum TurnEventDeliveryError {
    #[error("terminal event already sent")]
    TerminalAlreadySent,
    #[error("event delivery backpressure")]
    Backpressure,
    #[error("event receiver disconnected")]
    Disconnected,
}

pub trait TurnEventSink: Send + Sync {
    fn emit(&self, event: TurnEvent) -> Result<(), TurnEventDeliveryError>;
}
