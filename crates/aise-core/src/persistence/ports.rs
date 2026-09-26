use crate::core::IdempotencyKey;
use crate::core::{
    ActivationPreviewRequest, ActivationPreviewResult, CharacterCardInfo, PackInfo, PackSummaryInfo, StoryHistoryInfo,
    StoryId, StorySnapshotInfo, TurnResult, ValidationReport,
};
use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("resource not found")]
    NotFound,
    #[error("storage constraint violation: {constraint}")]
    ConstraintViolation { constraint: String },
    #[error("storage unavailable: {message}")]
    Unavailable { message: String },
}

#[derive(Debug, Clone)]
pub struct StoryHistoryQuery {
    pub turn_after: Option<u64>,
    pub turn_limit: usize,
}

#[derive(Debug, Clone)]
pub struct PackInput {
    pub content_type: PackContentType,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
pub enum PackContentType {
    Json,
    AisePack,
}

#[derive(Debug, Clone, Copy)]
pub enum PackExportFormat {
    Json,
    AisePack,
}

#[derive(Debug, Clone)]
pub enum PackExport {
    Json(Vec<u8>),
    AisePack(Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct StoryInstanceRequest {
    pub pack_id: String,
    pub player_id: String,
    pub player_role_id: String,
    pub role_profiles: Vec<RoleProfileSelection>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone)]
pub struct RoleProfileSelection {
    pub role_id: String,
    pub character_id: String,
    pub version: String,
    pub digest: String,
}

#[derive(Debug, Clone)]
pub struct StoryInstanceInfo {
    pub story_id: StoryId,
    pub base_revision: u64,
    pub created_at_ms: i64,
}

#[async_trait]
pub trait StoryRepository: Send + Sync {
    async fn story_exists(&self, story_id: &StoryId) -> Result<bool, PersistenceError>;
    async fn story_snapshot(&self, story_id: &StoryId) -> Result<StorySnapshotInfo, PersistenceError>;
    async fn story_history(
        &self,
        story_id: &StoryId,
        query: StoryHistoryQuery,
    ) -> Result<StoryHistoryInfo, PersistenceError>;
    async fn turn_result(
        &self,
        story_id: &StoryId,
        idempotency_key: &IdempotencyKey,
    ) -> Result<Option<TurnResult>, PersistenceError>;
}

#[async_trait]
pub trait StoryHistoryReader: Send + Sync {
    async fn read(&self, story_id: &StoryId, query: StoryHistoryQuery) -> Result<StoryHistoryInfo, PersistenceError>;
}

#[async_trait]
pub trait PackService: Send + Sync {
    async fn validate(&self, input: PackInput) -> Result<ValidationReport, PersistenceError>;
    async fn import(&self, input: PackInput) -> Result<PackInfo, PersistenceError>;
    async fn list(&self) -> Result<Vec<PackSummaryInfo>, PersistenceError>;
    async fn delete(&self, pack_id: &str) -> Result<bool, PersistenceError>;
    async fn export(&self, pack_id: &str, format: PackExportFormat) -> Result<PackExport, PersistenceError>;
}

#[async_trait]
pub trait CharacterCardService: Send + Sync {
    async fn validate(&self, bytes: Vec<u8>) -> Result<ValidationReport, PersistenceError>;
    async fn import(&self, bytes: Vec<u8>) -> Result<CharacterCardInfo, PersistenceError>;
    async fn list(&self) -> Result<Vec<CharacterCardInfo>, PersistenceError>;
}

#[async_trait]
pub trait StoryInstanceFactory: Send + Sync {
    async fn create(&self, request: StoryInstanceRequest) -> Result<StoryInstanceInfo, PersistenceError>;
}

#[async_trait]
pub trait ActivationPreviewService: Send + Sync {
    async fn preview(&self, request: ActivationPreviewRequest) -> Result<ActivationPreviewResult, PersistenceError>;
}
