use crate::core::turn_data::RetrievalAudience;
use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::ids::TopicKey;
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::{CharacterId, StoryRevision};
use crate::domain::knowledge::{KnowledgeKind, KnowledgeSource, KnowledgeSourceId};
use crate::domain::story_instance::snapshot::KnowledgeSnapshotRef;
use crate::persistence::store::StoreError;
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct KnowledgeFilter {
    pub audience: RetrievalAudience,
    pub knowledge_kinds: Vec<KnowledgeKind>,
    pub authorized_memory_owners: Vec<CharacterId>,
    pub max_item_bytes: usize,
}

#[derive(Debug, Clone)]
pub struct KnowledgeRecord {
    pub source_id: KnowledgeSourceId,
    pub kind: KnowledgeKind,
    pub content: BoundedText,
    pub salience: u8,
    pub source: KnowledgeSource,
    pub source_revision: StoryRevision,
    pub memory_owner: Option<CharacterId>,
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

#[async_trait]
pub trait KnowledgeReadPort: Send + Sync {
    async fn find_by_entities(&self, query: EntityKnowledgeQuery<'_>) -> Result<Vec<KnowledgeLookupHit>, StoreError>;

    async fn find_by_topics(&self, query: TopicKnowledgeQuery<'_>) -> Result<Vec<KnowledgeLookupHit>, StoreError>;
}
