use crate::domain::ids::StoryRevision;
use crate::domain::narrative::StoryTurn;
use crate::domain::story_instance::constraint::ActiveStoryConstraint;
use crate::domain::story_instance::info::StoryInfo;
use crate::domain::story_instance::role::StoryRole;
use crate::domain::story_instance::snapshot::StoryReadSnapshot;
use crate::domain::turn::SnapshotLimits;
use crate::turn::turn_contract::{CommittedTurnResult, IdempotencyKey, RequestDigest};
use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreSerializationErrorKind {
    InvalidStoryState,
    InvalidTurnResult,
    InvalidEventPayload,
    InvalidWorldState,
    InvalidRoleState,
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
    #[error("store limit exceeded: {limit}")]
    LimitExceeded { limit: &'static str },
    #[error("serialization error: {kind:?}")]
    Serialization { kind: StoreSerializationErrorKind },
    #[error("store unavailable")]
    Unavailable,
}

impl From<StoreError> for crate::turn::turn_error::TurnExecutionError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::RevisionConflict => Self::revision_conflict(None),
            StoreError::IdempotencyConflict => Self::idempotency_conflict(None),
            other => Self::new(
                crate::turn::turn_error::TurnFailureKind::Store,
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
    pub settings: crate::domain::story_instance::state::InstanceSettings,
    pub roles: std::collections::BTreeMap<crate::domain::ids::RoleId, StoryRole>,
    pub relationships: Vec<crate::domain::story_instance::state::RelationshipState>,
    pub knowledge: Vec<crate::domain::knowledge::KnowledgeEntry>,
    pub opening: crate::domain::asset::validation::BoundedText,
    pub narrative_state: crate::domain::narrative_graph::state::NarrativeRuntimeState,
    pub fact_values:
        std::collections::BTreeMap<crate::domain::asset::ids::FactKey, crate::domain::asset::validation::ScalarValue>,
    pub active_constraints: Vec<ActiveStoryConstraint>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone)]
pub struct StoryInstanceMeta {
    pub pack_id: crate::domain::asset::ids::PackId,
    pub roles: std::collections::BTreeMap<crate::domain::ids::RoleId, StoryRole>,
}

#[derive(Debug, Clone)]
pub struct TurnCommitSpec {
    pub story_id: crate::domain::ids::StoryId,
    pub base_revision: StoryRevision,
    pub expected_graph_revision: u64,
    pub turn: StoryTurn,
    pub changes: crate::turn::turn_validation::ValidatedChangeSet,
    pub idempotency_key: IdempotencyKey,
    pub request_digest: RequestDigest,
    pub outbox: Vec<OutboxRecord>,
    pub llm_calls: Vec<crate::turn::turn_contract::LlmCallUsage>,
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
