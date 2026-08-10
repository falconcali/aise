use crate::domain::ids::{StoryId, StoryRevision, TurnId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::time::Instant;
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub const MAX_PLAYER_INPUT_CHARS: usize = 4096;
pub const MAX_IDEMPOTENCY_KEY_CHARS: usize = 128;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum TurnRequestError {
    #[error("idempotency key must not be empty")]
    EmptyIdempotencyKey,
    #[error("idempotency key is {actual} chars, maximum {maximum}")]
    IdempotencyKeyTooLong { actual: usize, maximum: usize },
    #[error("player input must not be empty")]
    EmptyPlayerInput,
    #[error("player input is {actual} chars, maximum {maximum}")]
    PlayerInputTooLong { actual: usize, maximum: usize },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
    pub fn try_new(value: impl Into<String>) -> Result<Self, TurnRequestError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(TurnRequestError::EmptyIdempotencyKey);
        }
        let char_count = value.chars().count();
        if char_count > MAX_IDEMPOTENCY_KEY_CHARS {
            return Err(TurnRequestError::IdempotencyKeyTooLong {
                actual: char_count,
                maximum: MAX_IDEMPOTENCY_KEY_CHARS,
            });
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for IdempotencyKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestDigest(String);

impl RequestDigest {
    fn from_canonical_input(input: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(input.as_bytes());
        let hex = hasher.finalize().iter().fold(String::new(), |mut out, byte| {
            use std::fmt::Write;
            let _ = write!(out, "{byte:02x}");
            out
        });
        Self(hex)
    }

    pub fn from_stored(hex: String) -> Self {
        Self(hex)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct TurnIdentity {
    story_id: StoryId,
    turn_id: TurnId,
    idempotency_key: IdempotencyKey,
    started_at_ms: i64,
}

impl TurnIdentity {
    pub fn new(story_id: StoryId, turn_id: TurnId, idempotency_key: IdempotencyKey, started_at_ms: i64) -> Self {
        Self {
            story_id,
            turn_id,
            idempotency_key,
            started_at_ms,
        }
    }

    pub fn story_id(&self) -> &StoryId {
        &self.story_id
    }

    pub fn turn_id(&self) -> &TurnId {
        &self.turn_id
    }

    pub fn idempotency_key(&self) -> &IdempotencyKey {
        &self.idempotency_key
    }

    pub fn started_at_ms(&self) -> i64 {
        self.started_at_ms
    }
}

#[derive(Debug, Clone)]
pub struct TurnRequest {
    player_input: String,
    request_digest: RequestDigest,
}

impl TurnRequest {
    pub fn try_new(player_input: String) -> Result<Self, TurnRequestError> {
        let normalized = player_input.trim().to_string();
        let char_count = normalized.chars().count();
        if char_count == 0 {
            return Err(TurnRequestError::EmptyPlayerInput);
        }
        if char_count > MAX_PLAYER_INPUT_CHARS {
            return Err(TurnRequestError::PlayerInputTooLong {
                actual: char_count,
                maximum: MAX_PLAYER_INPUT_CHARS,
            });
        }
        let request_digest = RequestDigest::from_canonical_input(&normalized);
        Ok(Self {
            player_input: normalized,
            request_digest,
        })
    }

    pub fn player_input(&self) -> &str {
        &self.player_input
    }

    pub fn request_digest(&self) -> &RequestDigest {
        &self.request_digest
    }
}

#[derive(Debug, Clone)]
pub struct ExecuteTurnSpec {
    pub story_id: StoryId,
    pub idempotency_key: IdempotencyKey,
    pub player_input: String,
    pub cancellation: TurnCancellation,
}

#[derive(Debug, Clone)]
pub struct ValidatedExecuteTurnSpec {
    story_id: StoryId,
    idempotency_key: IdempotencyKey,
    request: TurnRequest,
    cancellation: TurnCancellation,
}

impl ValidatedExecuteTurnSpec {
    pub fn story_id(&self) -> &StoryId {
        &self.story_id
    }

    pub fn idempotency_key(&self) -> &IdempotencyKey {
        &self.idempotency_key
    }

    pub fn request(&self) -> &TurnRequest {
        &self.request
    }

    pub fn cancellation(&self) -> &TurnCancellation {
        &self.cancellation
    }

    pub fn into_parts(self) -> (StoryId, IdempotencyKey, TurnRequest, TurnCancellation) {
        (self.story_id, self.idempotency_key, self.request, self.cancellation)
    }
}

impl ExecuteTurnSpec {
    pub fn try_into_validated(self) -> Result<ValidatedExecuteTurnSpec, TurnRequestError> {
        let request = TurnRequest::try_new(self.player_input)?;
        Ok(ValidatedExecuteTurnSpec {
            story_id: self.story_id,
            idempotency_key: self.idempotency_key,
            request,
            cancellation: self.cancellation,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnPhase {
    Created,
    Initialized,
    Prepared,
    Planned,
    ContextReady,
    ProposalReady,
    RepairRequired,
    ReadyToCommit,
    Committed,
    Failed,
    Cancelled,
    Conflict,
}

impl TurnPhase {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TurnPhase::Committed | TurnPhase::Failed | TurnPhase::Cancelled | TurnPhase::Conflict
        )
    }
}

#[derive(Debug, Clone)]
pub struct TurnCancellation {
    token: CancellationToken,
}

impl TurnCancellation {
    pub fn new() -> Self {
        Self {
            token: CancellationToken::new(),
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    pub fn token(&self) -> &CancellationToken {
        &self.token
    }

    pub fn child(&self) -> Self {
        Self {
            token: self.token.child_token(),
        }
    }

    pub fn cancel(&self) {
        self.token.cancel();
    }
}

impl Default for TurnCancellation {
    fn default() -> Self {
        Self::new()
    }
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmUsageAggregate {
    pub llm_calls: u32,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageAccuracy {
    Exact,
    Estimated,
}

impl UsageAccuracy {
    pub fn as_str(&self) -> &'static str {
        match self {
            UsageAccuracy::Exact => "exact",
            UsageAccuracy::Estimated => "estimated",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    Length,
    ContentFilter,
    ToolCalls,
    Other(String),
}

impl FinishReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            FinishReason::Stop => "stop",
            FinishReason::Length => "length",
            FinishReason::ContentFilter => "content_filter",
            FinishReason::ToolCalls => "tool_calls",
            FinishReason::Other(_) => "other",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmCharge {
    pub provider: String,
    pub model: String,
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub amount_minor: i64,
    pub price_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmTokenUsage {
    pub input_tokens: u64,
    pub cached_input_tokens: Option<u64>,
    pub output_tokens: u64,
    pub reasoning_tokens: Option<u64>,
    pub total_tokens: u64,
    pub accuracy: UsageAccuracy,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LlmCallId(String);

impl LlmCallId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for LlmCallId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmCallPurpose {
    WriterPlan,
    ContextRetrieval,
    CharacterThink,
    StoryGeneration,
    StoryRepair,
    Embedding,
}

impl LlmCallPurpose {
    pub fn as_str(&self) -> &'static str {
        match self {
            LlmCallPurpose::WriterPlan => "writer_plan",
            LlmCallPurpose::ContextRetrieval => "context_retrieval",
            LlmCallPurpose::CharacterThink => "character_think",
            LlmCallPurpose::StoryGeneration => "story_generation",
            LlmCallPurpose::StoryRepair => "story_repair",
            LlmCallPurpose::Embedding => "embedding",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmCallUsage {
    pub call_id: LlmCallId,
    pub purpose: LlmCallPurpose,
    pub provider: String,
    pub model: String,
    pub input_tokens: u64,
    pub cached_input_tokens: Option<u64>,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub accuracy: UsageAccuracy,
    pub pricing_version: Option<String>,
    pub charge: Option<LlmCharge>,
    pub finish_reason: Option<FinishReason>,
}

#[derive(Debug, Clone, Default)]
pub struct LlmUsageLedger {
    calls: Vec<LlmCallUsage>,
    aggregate: LlmUsageAggregate,
}

impl LlmUsageLedger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, usage: LlmCallUsage) -> Result<(), crate::core::turn_error::TurnExecutionError> {
        self.aggregate.llm_calls = self.aggregate.llm_calls.saturating_add(1);
        self.aggregate.input_tokens = self.aggregate.input_tokens.saturating_add(usage.input_tokens);
        self.aggregate.output_tokens = self.aggregate.output_tokens.saturating_add(usage.output_tokens);
        self.aggregate.total_tokens = self.aggregate.total_tokens.saturating_add(usage.total_tokens);
        self.calls.push(usage);
        Ok(())
    }

    pub fn calls(&self) -> &[LlmCallUsage] {
        &self.calls
    }

    pub fn aggregate(&self) -> LlmUsageAggregate {
        self.aggregate
    }

    pub fn is_empty(&self) -> bool {
        self.calls.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommittedTurnResult {
    pub turn_id: TurnId,
    pub story_revision: StoryRevision,
    pub story_text: String,
    pub llm_usage: LlmUsageAggregate,
    pub llm_calls: Vec<LlmCallUsage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmCallStatus {
    Succeeded,
    Cancelled,
    TurnDeadlineExceeded,
    ProviderTimeout,
    QueueTimeout,
    RateLimited,
    TokenBudgetExceeded,
    ProviderRejected,
    TransportFailed,
    ProtocolFailed,
    ResponseLimitExceeded,
}

impl LlmCallStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            LlmCallStatus::Succeeded => "succeeded",
            LlmCallStatus::Cancelled => "cancelled",
            LlmCallStatus::TurnDeadlineExceeded => "turn_deadline_exceeded",
            LlmCallStatus::ProviderTimeout => "provider_timeout",
            LlmCallStatus::QueueTimeout => "queue_timeout",
            LlmCallStatus::RateLimited => "rate_limited",
            LlmCallStatus::TokenBudgetExceeded => "token_budget_exceeded",
            LlmCallStatus::ProviderRejected => "provider_rejected",
            LlmCallStatus::TransportFailed => "transport_failed",
            LlmCallStatus::ProtocolFailed => "protocol_failed",
            LlmCallStatus::ResponseLimitExceeded => "response_limit_exceeded",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LlmBudgetReservation {
    call_id: LlmCallId,
    reserved_input_tokens: u64,
    reserved_output_tokens: u64,
}

impl LlmBudgetReservation {
    pub fn new(call_id: LlmCallId, reserved_input_tokens: u64, reserved_output_tokens: u64) -> Self {
        Self {
            call_id,
            reserved_input_tokens,
            reserved_output_tokens,
        }
    }

    pub fn call_id(&self) -> &LlmCallId {
        &self.call_id
    }

    pub fn reserved_input_tokens(&self) -> u64 {
        self.reserved_input_tokens
    }

    pub fn reserved_output_tokens(&self) -> u64 {
        self.reserved_output_tokens
    }
}
