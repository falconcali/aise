use crate::domain::ids::{StoryId, TurnId};
use crate::error::AiseError;
use sha2::{Digest, Sha256};
use std::fmt;
use std::time::Instant;
use tokio_util::sync::CancellationToken;

pub const MAX_PLAYER_INPUT_CHARS: usize = 4096;

#[derive(Debug, Clone)]
pub struct ExecuteTurnSpec {
    pub story_id: StoryId,
    pub idempotency_key: IdempotencyKey,
    pub player_input: String,
    pub cancellation: TurnCancellation,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
    pub fn try_new(value: String) -> Result<Self, AiseError> {
        if value.trim().is_empty() {
            return Err(AiseError::InvalidRequest("idempotency key must not be empty".into()));
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
        let hex: String = hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect();
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
    pub fn new(
        story_id: StoryId,
        turn_id: TurnId,
        idempotency_key: IdempotencyKey,
        started_at_ms: i64,
    ) -> Result<Self, AiseError> {
        if story_id.as_str().is_empty() {
            return Err(AiseError::InvalidRequest("story_id must not be empty".into()));
        }
        if turn_id.as_str().is_empty() {
            return Err(AiseError::InvalidRequest("turn_id must not be empty".into()));
        }
        if idempotency_key.as_str().is_empty() {
            return Err(AiseError::InvalidRequest("idempotency_key must not be empty".into()));
        }
        Ok(Self {
            story_id,
            turn_id,
            idempotency_key,
            started_at_ms,
        })
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
    pub fn try_new(player_input: String) -> Result<Self, AiseError> {
        let normalized = player_input.trim().to_string();
        if normalized.is_empty() {
            return Err(AiseError::InvalidRequest("player input must not be empty".into()));
        }
        if normalized.chars().count() > MAX_PLAYER_INPUT_CHARS {
            return Err(AiseError::InvalidRequest(format!(
                "player input exceeds {MAX_PLAYER_INPUT_CHARS} chars"
            )));
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

#[derive(Debug, Clone)]
pub struct CommittedTurnResult {
    pub turn_id: TurnId,
    pub story_text: String,
}
