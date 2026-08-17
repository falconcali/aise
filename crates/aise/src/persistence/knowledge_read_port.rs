use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::ids::TopicKey;
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::RoleId;
use crate::domain::knowledge::{KnowledgeKind, KnowledgeSource, KnowledgeSourceId, RetrievalHint};
use crate::domain::story_instance::snapshot::KnowledgeSnapshotRef;
use crate::domain::turn::RetrievalAudience;
use crate::persistence::store::StoreError;
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct KnowledgeFilter {
    pub audience: RetrievalAudience,
    pub knowledge_kinds: Vec<KnowledgeKind>,
    pub authorized_memory_owners: Vec<RoleId>,
    pub max_item_bytes: usize,
}

#[derive(Debug, Clone)]
pub struct KnowledgeRecord {
    pub source_id: KnowledgeSourceId,
    pub kind: KnowledgeKind,
    pub content: BoundedText,
    pub salience: u8,
    pub source: KnowledgeSource,
    pub memory_owner: Option<RoleId>,
}

#[derive(Debug, Clone)]
pub struct KnowledgeLookupHit {
    pub record: KnowledgeRecord,
    pub matches: Vec<crate::domain::knowledge::KnowledgeIndexMatch>,
}

#[derive(Debug, Clone)]
pub struct EntityKnowledgeQuery<'a> {
    pub snapshot: &'a KnowledgeSnapshotRef,
    pub filter: &'a KnowledgeFilter,
    pub entities: &'a [KnowledgeEntity],
    pub limit: usize,
}

#[derive(Debug, Clone)]
pub struct TopicKnowledgeQuery<'a> {
    pub snapshot: &'a KnowledgeSnapshotRef,
    pub filter: &'a KnowledgeFilter,
    pub topics: &'a [TopicKey],
    pub limit: usize,
}

#[derive(Debug, Clone)]
pub struct SourceKnowledgeQuery<'a> {
    pub snapshot: &'a KnowledgeSnapshotRef,
    pub filter: &'a KnowledgeFilter,
    pub source_ids: &'a [KnowledgeSourceId],
    pub limit: usize,
}

#[derive(Debug, Clone)]
pub struct KnowledgeIndexQuery<'a> {
    pub snapshot: &'a KnowledgeSnapshotRef,
    pub knowledge_kinds: &'a [KnowledgeKind],
    pub limit: usize,
}

#[derive(Debug, Clone)]
pub struct KnowledgeIndexRecord {
    pub source_id: KnowledgeSourceId,
    pub kind: KnowledgeKind,
    pub retrieval_hint: Option<RetrievalHint>,
}

#[async_trait]
pub trait KnowledgeReadPort: Send + Sync {
    async fn find_by_entities(&self, query: EntityKnowledgeQuery<'_>) -> Result<Vec<KnowledgeLookupHit>, StoreError>;

    async fn find_by_topics(&self, query: TopicKnowledgeQuery<'_>) -> Result<Vec<KnowledgeLookupHit>, StoreError>;

    async fn find_by_source_ids(&self, query: SourceKnowledgeQuery<'_>) -> Result<Vec<KnowledgeRecord>, StoreError>;

    async fn list_index(&self, query: KnowledgeIndexQuery<'_>) -> Result<Vec<KnowledgeIndexRecord>, StoreError>;
}
