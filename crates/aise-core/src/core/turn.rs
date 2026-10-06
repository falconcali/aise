use super::{IdempotencyKey, StoryId, WorldChange};
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
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

    pub async fn cancelled(&self) {
        self.0.cancelled().await;
    }
}

impl Default for TurnCancellation {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TurnEvent {
    StageStarted { stage: String },
    Committed,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnNumber(u64);

impl TurnNumber {
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn increment(&self) -> Self {
        Self(self.0 + 1)
    }

    pub fn value(&self) -> u64 {
        self.0
    }
}

impl Default for TurnNumber {
    fn default() -> Self {
        Self(0)
    }
}

impl Display for TurnNumber {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnSegment(String);

impl TurnSegment {
    pub fn new(text: String) -> Self {
        Self(text)
    }

    pub fn text(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl Display for TurnSegment {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnEvaluation {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TurnStatus {
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    pub turn_number: TurnNumber,
    pub idempotency_key: IdempotencyKey,
    pub player_contribution: PlayerContribution,
    pub turn_segment: TurnSegment,
    pub world_change: WorldChange,
    pub turn_evaluation: TurnEvaluation,
    pub turn_status: TurnStatus,
}
