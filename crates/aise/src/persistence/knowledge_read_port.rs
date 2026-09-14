use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::RoleId;
use crate::domain::knowledge::activation::{ActivationRuleVersion, KnowledgeActivationRule};
use crate::domain::knowledge::{KnowledgeKind, KnowledgeSource, KnowledgeSourceId, RetrievalHint};
use crate::domain::story_instance::snapshot::KnowledgeSnapshotRef;
use crate::domain::turn::KnowledgeDelivery;
use crate::persistence::store::StoreError;
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct KnowledgeFilter {
    pub delivery: KnowledgeDelivery,
    pub knowledge_kinds: Vec<KnowledgeKind>,
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
    pub activation: Option<KnowledgeActivationRule>,
    pub activation_rule_version: Option<ActivationRuleVersion>,
}

#[derive(Debug, Clone)]
pub struct SourceKnowledgeQuery<'a> {
    pub snapshot: &'a KnowledgeSnapshotRef,
    pub filter: &'a KnowledgeFilter,
    pub source_ids: &'a [KnowledgeSourceId],
    pub limit: usize,
}

#[derive(Debug, Clone)]
pub struct OwnerMemoryQuery<'a> {
    pub snapshot: &'a KnowledgeSnapshotRef,
    pub owner: &'a RoleId,
    pub limit: usize,
    pub max_item_bytes: usize,
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
    pub retrieval_hint: RetrievalHint,
}

#[async_trait]
pub trait KnowledgeReadPort: Send + Sync {
    async fn find_by_source_ids(&self, query: SourceKnowledgeQuery<'_>) -> Result<Vec<KnowledgeRecord>, StoreError>;

    async fn find_memories_by_owner(&self, query: OwnerMemoryQuery<'_>) -> Result<Vec<KnowledgeRecord>, StoreError>;

    async fn list_index(&self, query: KnowledgeIndexQuery<'_>) -> Result<Vec<KnowledgeIndexRecord>, StoreError>;
}
