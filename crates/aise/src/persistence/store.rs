use crate::core::turn_contract::{
    CommittedTurnResult, IdempotencyKey, LlmUsageAggregate, RequestDigest, StoryRevision,
};
use crate::core::turn_data::{SnapshotLimits, StoryReadSnapshot};
use crate::core::turn_validation::StateChange;
use crate::domain::character::CharacterState;
use crate::domain::ids::{CharacterId, StoryId, TurnId};
use crate::domain::memory::MemoryEntry;
use crate::domain::narrative::{StoryEvent, StoryTurn};
use crate::domain::world::WorldState;
use crate::error::AiseError;
use async_trait::async_trait;

#[async_trait]
pub trait Store: Send + Sync {
    async fn load_story_snapshot(
        &self,
        story_id: &StoryId,
        limits: SnapshotLimits,
    ) -> Result<Option<StoryReadSnapshot>, AiseError>;

    async fn create_story(
        &self,
        story_id: &StoryId,
        player_character_id: Option<&CharacterId>,
        created_at: i64,
    ) -> Result<(), AiseError>;

    async fn find_committed_turn(
        &self,
        story_id: &StoryId,
        idempotency_key: &IdempotencyKey,
    ) -> Result<Option<StoredTurnOutcome>, AiseError>;

    async fn commit_turn(&self, commit: &TurnCommit) -> Result<CommittedTurnResult, AiseError>;
}

#[derive(Debug, Clone)]
pub struct StoredTurnOutcome {
    pub request_digest: RequestDigest,
    pub result: CommittedTurnResult,
}

#[derive(Debug, Clone)]
pub struct TurnCommit {
    pub story_id: StoryId,
    pub turn: StoryTurn,
    pub events: Vec<StoryEvent>,
    pub characters: Vec<CharacterState>,
    pub world: StateChange<WorldState>,
    pub memory: Vec<MemoryEntry>,
    pub base_revision: StoryRevision,
    pub idempotency_key: IdempotencyKey,
    pub request_digest: RequestDigest,
    pub player_character_id: Option<CharacterId>,
    pub outbox: Vec<OutboxRecord>,
    pub llm_usage: LlmUsageAggregate,
}

#[derive(Debug, Clone)]
pub struct OutboxRecord {
    pub id: String,
    pub story_id: StoryId,
    pub turn_id: TurnId,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub created_at: i64,
}
