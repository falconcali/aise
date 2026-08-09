use crate::core::turn_contract::{CommittedTurnResult, IdempotencyKey, RequestDigest};
use crate::core::turn_data::SnapshotLimits;
use crate::core::turn_validation::StateChange;
use crate::domain::ids::{CharacterId, StoryRevision};
use crate::domain::narrative::{StoryEvent, StorySummary, StoryTurn};
use crate::domain::story_instance::constraint::ActiveStoryConstraint;
use crate::domain::story_instance::info::StoryInfo;
use crate::domain::story_instance::snapshot::StoryReadSnapshot;
use crate::domain::story_instance::state::CurrentScene;
use crate::domain::world::WorldState;
use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreSerializationErrorKind {
    InvalidStoryState,
    InvalidTurnResult,
    InvalidEventPayload,
    InvalidWorldState,
    InvalidCharacterState,
    InvalidMemory,
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("story not found")]
    NotFound,
    #[error("revision conflict")]
    RevisionConflict,
    #[error("idempotency conflict")]
    IdempotencyConflict,
    #[error("constraint violation: {constraint}")]
    ConstraintViolation { constraint: String },
    #[error("serialization error: {kind:?}")]
    Serialization { kind: StoreSerializationErrorKind },
    #[error("store unavailable")]
    Unavailable,
}

impl From<StoreError> for crate::core::turn_error::TurnExecutionError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::RevisionConflict => Self::revision_conflict(None),
            StoreError::IdempotencyConflict => Self::idempotency_conflict(None),
            other => Self::new(
                crate::core::turn_error::TurnFailureKind::Store,
                "store_error",
                None,
                other.to_string(),
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub struct StoredTurnOutcome {
    pub request_digest: RequestDigest,
    pub result: CommittedTurnResult,
}

#[derive(Debug, Clone)]
pub struct MaterializedStoryInstanceSpec {
    pub story_id: crate::domain::ids::StoryId,
    pub pack: crate::domain::asset::frozen_ref::FrozenStoryPackRef,
    pub bindings: std::collections::BTreeMap<
        crate::domain::asset::ids::StoryRoleKey,
        crate::domain::story_instance::binding::RoleBinding,
    >,
    pub characters: std::collections::BTreeMap<
        crate::domain::ids::CharacterId,
        crate::domain::story_instance::state::CharacterInstanceState,
    >,
    pub relationships: Vec<crate::domain::story_instance::state::RelationshipState>,
    pub facts: Vec<crate::domain::knowledge::fact::WorldFact>,
    pub rumors: Vec<crate::domain::knowledge::rumor::SharedRumor>,
    pub memories: Vec<crate::domain::knowledge::memory::MemoryEntry>,
    pub scene: CurrentScene,
    pub opening: crate::domain::asset::validation::BoundedText,
    pub narrative_state: crate::domain::narrative_graph::state::NarrativeRuntimeState,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone)]
pub struct StoryInstanceMeta {
    pub pack_id: crate::domain::asset::ids::PackId,
    pub bindings: std::collections::BTreeMap<
        crate::domain::asset::ids::StoryRoleKey,
        crate::domain::story_instance::binding::RoleBinding,
    >,
    pub characters: std::collections::BTreeMap<
        crate::domain::ids::CharacterId,
        crate::domain::story_instance::state::CharacterInstanceState,
    >,
}

#[derive(Debug, Clone)]
pub struct TurnCommitSpec {
    pub story_id: crate::domain::ids::StoryId,
    pub turn: StoryTurn,
    pub events: Vec<StoryEvent>,
    pub character_changes: Vec<crate::core::turn_validation::CharacterStateChange>,
    pub world_change: StateChange<WorldState>,
    pub memory_changes: Vec<crate::core::turn_validation::MemoryStateChange>,
    pub scene_change: StateChange<CurrentScene>,
    pub constraint_change: StateChange<Vec<ActiveStoryConstraint>>,
    pub summary_change: StateChange<StorySummary>,
    pub base_revision: StoryRevision,
    pub idempotency_key: IdempotencyKey,
    pub request_digest: RequestDigest,
    pub player_character_id: Option<CharacterId>,
    pub outbox: Vec<OutboxRecord>,
    pub llm_calls: Vec<crate::core::turn_contract::LlmCallUsage>,
}

#[derive(Debug, Clone)]
pub struct OutboxRecord {
    pub id: String,
    pub story_id: crate::domain::ids::StoryId,
    pub turn_id: crate::domain::ids::TurnId,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub created_at: i64,
}

#[async_trait]
pub trait Store: Send + Sync {
    async fn create_story_instance(&self, spec: &MaterializedStoryInstanceSpec) -> Result<StoryInfo, StoreError>;
    async fn get_story(&self, story_id: &crate::domain::ids::StoryId) -> Result<Option<StoryInfo>, StoreError>;
    async fn load_story_snapshot(
        &self,
        story_id: &crate::domain::ids::StoryId,
        limits: SnapshotLimits,
    ) -> Result<StoryReadSnapshot, StoreError>;
    async fn load_story_instance_meta(
        &self,
        story_id: &crate::domain::ids::StoryId,
    ) -> Result<Option<StoryInstanceMeta>, StoreError>;
    async fn find_committed_turn(
        &self,
        story_id: &crate::domain::ids::StoryId,
        idempotency_key: &IdempotencyKey,
    ) -> Result<Option<StoredTurnOutcome>, StoreError>;
    async fn commit_turn(&self, spec: &TurnCommitSpec) -> Result<CommittedTurnResult, StoreError>;
}
