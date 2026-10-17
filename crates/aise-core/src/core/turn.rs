use super::{BoundedText, IdempotencyKey, KnowledgeChange, StoryId};
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::time::Instant;
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
    TurnStarted,
    TurnPipeline { stage: String },
    TurnCommitted,
}

pub trait TurnEventSink: Send + Sync {
    fn emit(&self, event: TurnEvent);
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TurnStatus {
    Pass,
    Choice,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnCheckEvaluation {
    pub name: String,
    pub passed: bool,
    pub issue: Option<BoundedText>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TurnScoreEvaluation {
    pub name: String,
    pub min_score: f64,
    pub max_score: f64,
    pub target_score: f64,
    pub score: f64,
    pub issue: Option<BoundedText>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TurnChoiceEvaluation {
    pub selected_proposal_version: u32,
    pub target_gap_mse: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TurnEvaluation {
    pub proposal_version: u32,
    pub repair_rounds: u32,
    pub checks: Vec<TurnCheckEvaluation>,
    pub scores: Vec<TurnScoreEvaluation>,
    pub choice: Option<TurnChoiceEvaluation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    pub turn_number: TurnNumber,
    pub idempotency_key: IdempotencyKey,
    pub player_contribution: PlayerContribution,
    pub turn_segment: TurnSegment,
    pub knowledge_change: KnowledgeChange,
    pub turn_evaluation: TurnEvaluation,
    pub turn_status: TurnStatus,
}

#[cfg(test)]
#[path = "tests/turn_tests.rs"]
mod tests;
