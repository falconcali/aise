use crate::core::turn_data::RetrievalAudience;
use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::ids::{
    CanonicalEventKey, EntityKey, LocationKey, NarrativeNodeKey, SceneKey, StoryRoleKey, TopicKey,
};
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::{CharacterId, FactId, MemoryId, StoryRevision};
use crate::domain::knowledge::{KnowledgeKind, KnowledgeSource, KnowledgeSourceId};
use crate::persistence::knowledge_read_port::{
    EntityKnowledgeQuery, KnowledgeFilter, KnowledgeReadPort, KnowledgeRecord, TopicKnowledgeQuery,
};
use crate::persistence::sqlite_error::SqliteStoreError;
use crate::persistence::sqlite_store::SqliteStore;
use crate::persistence::store::StoreError;
use async_trait::async_trait;
use sqlx::Row;
use std::sync::Arc;

#[async_trait]
impl KnowledgeReadPort for SqliteStore {
    async fn find_by_entities(&self, query: EntityKnowledgeQuery<'_>) -> Result<Vec<KnowledgeRecord>, StoreError> {
        if query.entities.is_empty() || query.limit == 0 {
            return Ok(Vec::new());
        }
        let mut tx = self.pool().begin().await.map_err(SqliteStoreError::from)?;
        verify_snapshot(&mut tx, query.snapshot).await?;
        let kinds = kind_filter_sql(&query.filter.knowledge_kinds);
        let mut records = Vec::new();
        for entity in query.entities {
            let (entity_kind, entity_key) = entity_parts(entity);
            let rows = sqlx::query(&format!(
                "SELECT e.source_id, e.knowledge_kind, e.memory_owner_character_id, e.content, e.salience, \
                     e.source_json, e.source_revision \
                     FROM knowledge_entries e \
                     INNER JOIN knowledge_entry_entities m \
                       ON e.story_id = m.story_id \
                      AND e.knowledge_kind = m.knowledge_kind \
                      AND e.source_id = m.source_id \
                     WHERE e.story_id = ? AND m.entity_kind = ? AND m.entity_key = ? \
                       AND e.knowledge_kind IN ({kinds}) \
                     ORDER BY e.source_id ASC \
                     LIMIT ?"
            ))
            .bind(query.snapshot.story_id.as_str())
            .bind(entity_kind)
            .bind(entity_key)
            .bind(query.limit as i64)
            .fetch_all(&mut *tx)
            .await
            .map_err(SqliteStoreError::from)?;
            for row in rows {
                if let Some(record) = materialize_row(row, query.filter)? {
                    if !records.iter().any(|existing: &KnowledgeRecord| {
                        existing.source_id == record.source_id && existing.kind == record.kind
                    }) {
                        records.push(record);
                    }
                }
                if records.len() >= query.limit {
                    break;
                }
            }
            if records.len() >= query.limit {
                break;
            }
        }
        records.truncate(query.limit);
        tx.commit().await.map_err(SqliteStoreError::from)?;
        Ok(records)
    }

    async fn find_by_topics(&self, query: TopicKnowledgeQuery<'_>) -> Result<Vec<KnowledgeRecord>, StoreError> {
        if query.topics.is_empty() || query.limit == 0 {
            return Ok(Vec::new());
        }
        let mut tx = self.pool().begin().await.map_err(SqliteStoreError::from)?;
        verify_snapshot(&mut tx, query.snapshot).await?;
        let kinds = kind_filter_sql(&query.filter.knowledge_kinds);
        let mut records = Vec::new();
        for topic in query.topics {
            let rows = sqlx::query(&format!(
                "SELECT e.source_id, e.knowledge_kind, e.memory_owner_character_id, e.content, e.salience, \
                     e.source_json, e.source_revision \
                     FROM knowledge_entries e \
                     INNER JOIN knowledge_entry_topics m \
                       ON e.story_id = m.story_id \
                      AND e.knowledge_kind = m.knowledge_kind \
                      AND e.source_id = m.source_id \
                     WHERE e.story_id = ? AND m.topic_key = ? \
                       AND e.knowledge_kind IN ({kinds}) \
                     ORDER BY e.source_id ASC \
                     LIMIT ?"
            ))
            .bind(query.snapshot.story_id.as_str())
            .bind(topic.as_str())
            .bind(query.limit as i64)
            .fetch_all(&mut *tx)
            .await
            .map_err(SqliteStoreError::from)?;
            for row in rows {
                if let Some(record) = materialize_row(row, query.filter)? {
                    if !records.iter().any(|existing: &KnowledgeRecord| {
                        existing.source_id == record.source_id && existing.kind == record.kind
                    }) {
                        records.push(record);
                    }
                }
                if records.len() >= query.limit {
                    break;
                }
            }
            if records.len() >= query.limit {
                break;
            }
        }
        records.truncate(query.limit);
        tx.commit().await.map_err(SqliteStoreError::from)?;
        Ok(records)
    }
}

#[async_trait]
impl KnowledgeReadPort for Arc<SqliteStore> {
    async fn find_by_entities(&self, query: EntityKnowledgeQuery<'_>) -> Result<Vec<KnowledgeRecord>, StoreError> {
        KnowledgeReadPort::find_by_entities(&**self, query).await
    }

    async fn find_by_topics(&self, query: TopicKnowledgeQuery<'_>) -> Result<Vec<KnowledgeRecord>, StoreError> {
        KnowledgeReadPort::find_by_topics(&**self, query).await
    }
}

async fn verify_snapshot(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    snapshot: &crate::domain::story_instance::snapshot::KnowledgeSnapshotRef,
) -> Result<(), StoreError> {
    let row: Option<(i64, String)> = sqlx::query_as(
        "SELECT s.revision, p.digest \
         FROM stories s \
         INNER JOIN story_instances i ON i.story_id = s.id \
         INNER JOIN story_packs p ON p.pack_id = i.pack_id \
         WHERE s.id = ?",
    )
    .bind(snapshot.story_id.as_str())
    .fetch_optional(&mut **tx)
    .await
    .map_err(SqliteStoreError::from)?;
    let Some((revision, digest)) = row else {
        return Err(StoreError::NotFound);
    };
    if revision as u64 != snapshot.base_revision.get() {
        return Err(StoreError::RevisionConflict);
    }
    if digest != snapshot.pack_digest.to_string() {
        return Err(StoreError::RevisionConflict);
    }
    Ok(())
}

fn kind_filter_sql(kinds: &[KnowledgeKind]) -> String {
    if kinds.is_empty() {
        return "'fact','rumor','memory'".into();
    }
    kinds
        .iter()
        .map(|kind| match kind {
            KnowledgeKind::Fact => "'fact'",
            KnowledgeKind::Rumor => "'rumor'",
            KnowledgeKind::Memory => "'memory'",
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn entity_parts(entity: &KnowledgeEntity) -> (&'static str, String) {
    match entity {
        KnowledgeEntity::World(key) => ("world", key.as_str().to_owned()),
        KnowledgeEntity::Role(key) => ("role", key.as_str().to_owned()),
        KnowledgeEntity::Character(id) => ("character", id.as_str().to_owned()),
        KnowledgeEntity::Location(key) => ("location", key.as_str().to_owned()),
        KnowledgeEntity::Scene(key) => ("scene", key.as_str().to_owned()),
        KnowledgeEntity::NarrativeNode(key) => ("narrative_node", key.as_str().to_owned()),
        KnowledgeEntity::Event(key) => ("event", key.as_str().to_owned()),
    }
}

fn materialize_row(
    row: sqlx::sqlite::SqliteRow,
    filter: &KnowledgeFilter,
) -> Result<Option<KnowledgeRecord>, StoreError> {
    let source_id_raw: String = row.get("source_id");
    let kind_raw: String = row.get("knowledge_kind");
    let owner_raw: Option<String> = row.get("memory_owner_character_id");
    let content_raw: String = row.get("content");
    let salience: i64 = row.get("salience");
    let source_json: String = row.get("source_json");
    let source_revision: i64 = row.get("source_revision");
    let kind = match kind_raw.as_str() {
        "fact" => KnowledgeKind::Fact,
        "rumor" => KnowledgeKind::Rumor,
        "memory" => KnowledgeKind::Memory,
        _ => {
            return Err(StoreError::Serialization {
                kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
            });
        }
    };
    let memory_owner = owner_raw.map(CharacterId::from);
    if !authorize(filter, kind, memory_owner.as_ref()) {
        return Ok(None);
    }
    let source_id = match kind {
        KnowledgeKind::Fact => KnowledgeSourceId::Fact(FactId::from(source_id_raw)),
        KnowledgeKind::Rumor => KnowledgeSourceId::Rumor(crate::domain::asset::ids::RumorId::from(source_id_raw)),
        KnowledgeKind::Memory => KnowledgeSourceId::Memory(MemoryId::from(source_id_raw)),
    };
    let source: KnowledgeSource = serde_json::from_str(&source_json).map_err(|_| StoreError::Serialization {
        kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
    })?;
    let content =
        BoundedText::try_new(content_raw, "knowledge_content", usize::MAX).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
        })?;
    Ok(Some(KnowledgeRecord {
        source_id,
        kind,
        content,
        entities: Vec::new(),
        topics: Vec::new(),
        salience: salience.clamp(0, 255) as u8,
        source,
        source_revision: StoryRevision::new(source_revision as u64),
        memory_owner,
    }))
}

fn authorize(filter: &KnowledgeFilter, kind: KnowledgeKind, owner: Option<&CharacterId>) -> bool {
    if !filter.knowledge_kinds.is_empty() && !filter.knowledge_kinds.contains(&kind) {
        return false;
    }
    match &filter.audience {
        RetrievalAudience::GlobalWriter => match kind {
            KnowledgeKind::Fact | KnowledgeKind::Rumor => true,
            KnowledgeKind::Memory => owner
                .map(|id| filter.allowed_writer_memory_owners.iter().any(|allowed| allowed == id))
                .unwrap_or(false),
        },
        RetrievalAudience::Character { character_id } => match kind {
            KnowledgeKind::Fact => false,
            KnowledgeKind::Rumor => true,
            KnowledgeKind::Memory => owner == Some(character_id),
        },
    }
}

impl SqliteStore {
    pub(crate) fn pool(&self) -> &sqlx::SqlitePool {
        self.pool_for_tests()
    }
}

pub(crate) fn _entity_key_anchor(
    _: &EntityKey,
    _: &StoryRoleKey,
    _: &LocationKey,
    _: &SceneKey,
    _: &NarrativeNodeKey,
    _: &CanonicalEventKey,
    _: &TopicKey,
) {
}
