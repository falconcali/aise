use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::RoleId;
use crate::domain::knowledge::{KnowledgeKind, KnowledgeSource, KnowledgeSourceId};
use crate::domain::story_instance::snapshot::KnowledgeSnapshotRef;
use crate::domain::turn::KnowledgeDelivery;
use crate::persistence::knowledge_read_port::{
    KnowledgeFilter, KnowledgeIndexQuery, KnowledgeIndexRecord, KnowledgeReadPort, KnowledgeRecord, OwnerMemoryQuery,
    SourceKnowledgeQuery,
};
use crate::persistence::sqlite_error::SqliteStoreError;
use crate::persistence::sqlite_store::SqliteStore;
use crate::persistence::store::{StoreError, StoreSerializationErrorKind};
use async_trait::async_trait;
use sqlx::{QueryBuilder, Row, Sqlite};
use std::sync::Arc;

#[async_trait]
impl KnowledgeReadPort for SqliteStore {
    async fn find_by_source_ids(&self, query: SourceKnowledgeQuery<'_>) -> Result<Vec<KnowledgeRecord>, StoreError> {
        load_by_source_ids(self.pool(), query).await
    }

    async fn find_memories_by_owner(&self, query: OwnerMemoryQuery<'_>) -> Result<Vec<KnowledgeRecord>, StoreError> {
        load_memories_by_owner(self.pool(), query).await
    }

    async fn list_index(&self, query: KnowledgeIndexQuery<'_>) -> Result<Vec<KnowledgeIndexRecord>, StoreError> {
        load_index(self.pool(), query).await
    }
}

#[async_trait]
impl KnowledgeReadPort for Arc<SqliteStore> {
    async fn find_by_source_ids(&self, query: SourceKnowledgeQuery<'_>) -> Result<Vec<KnowledgeRecord>, StoreError> {
        KnowledgeReadPort::find_by_source_ids(&**self, query).await
    }

    async fn find_memories_by_owner(&self, query: OwnerMemoryQuery<'_>) -> Result<Vec<KnowledgeRecord>, StoreError> {
        KnowledgeReadPort::find_memories_by_owner(&**self, query).await
    }

    async fn list_index(&self, query: KnowledgeIndexQuery<'_>) -> Result<Vec<KnowledgeIndexRecord>, StoreError> {
        KnowledgeReadPort::list_index(&**self, query).await
    }
}

async fn load_memories_by_owner(
    pool: &sqlx::SqlitePool,
    query: OwnerMemoryQuery<'_>,
) -> Result<Vec<KnowledgeRecord>, StoreError> {
    if query.max_item_bytes == 0 {
        return Err(StoreError::LimitExceeded {
            limit: "max_item_bytes",
        });
    }
    let mut tx = pool.begin().await.map_err(SqliteStoreError::from)?;
    verify_snapshot(&mut tx, query.snapshot).await?;
    if query.limit == 0 {
        tx.commit().await.map_err(SqliteStoreError::from)?;
        return Ok(Vec::new());
    }
    let rows = sqlx::query(
        "SELECT e.source_id, e.knowledge_kind, e.memory_owner_role_id, e.content,
         length(CAST(e.content AS BLOB)) AS content_bytes, e.salience, e.source_json, e.payload_json,
         length(CAST(e.payload_json AS BLOB)) AS payload_bytes
         FROM knowledge_entries e
         WHERE e.story_id = ?1 AND e.knowledge_kind = 'memory'
         AND e.memory_owner_role_id = ?2 ORDER BY e.source_id ASC LIMIT ?3",
    )
    .bind(query.snapshot.story_id.as_str())
    .bind(query.owner.as_str())
    .bind(i64::try_from(query.limit).map_err(|_| StoreError::LimitExceeded {
        limit: "knowledge_limit",
    })?)
    .fetch_all(&mut *tx)
    .await
    .map_err(SqliteStoreError::from)?;
    let records = rows
        .iter()
        .map(|row| materialize_row(row, query.max_item_bytes))
        .collect::<Result<Vec<_>, _>>()?;
    tx.commit().await.map_err(SqliteStoreError::from)?;
    Ok(records)
}

async fn load_by_source_ids(
    pool: &sqlx::SqlitePool,
    query: SourceKnowledgeQuery<'_>,
) -> Result<Vec<KnowledgeRecord>, StoreError> {
    validate_filter(query.filter)?;
    let mut tx = pool.begin().await.map_err(SqliteStoreError::from)?;
    verify_snapshot(&mut tx, query.snapshot).await?;
    if query.source_ids.is_empty() || query.limit == 0 {
        tx.commit().await.map_err(SqliteStoreError::from)?;
        return Ok(Vec::new());
    }
    let mut builder = QueryBuilder::<Sqlite>::new(
        "SELECT e.source_id, e.knowledge_kind, e.memory_owner_role_id, e.content, \
         length(CAST(e.content AS BLOB)) AS content_bytes, e.salience, e.source_json, e.payload_json, \
         length(CAST(e.payload_json AS BLOB)) AS payload_bytes \
         FROM knowledge_entries e WHERE e.story_id = ",
    );
    builder.push_bind(query.snapshot.story_id.as_str());
    builder.push(" AND e.knowledge_kind IN (");
    {
        let mut separated = builder.separated(", ");
        for kind in &query.filter.knowledge_kinds {
            separated.push_bind(kind_name(*kind));
        }
    }
    builder.push(")");
    builder.push(" AND (");
    for (index, source_id) in query.source_ids.iter().enumerate() {
        if index > 0 {
            builder.push(" OR ");
        }
        builder
            .push("(e.knowledge_kind = ")
            .push_bind(kind_name(source_id_kind(source_id)))
            .push(" AND e.source_id = ")
            .push_bind(source_id.as_str())
            .push(")");
    }
    builder.push(")");
    push_authorization(&mut builder, query.filter);
    builder.push(" ORDER BY e.source_id ASC LIMIT ");
    builder.push_bind(i64::try_from(query.limit).map_err(|_| StoreError::LimitExceeded {
        limit: "knowledge_limit",
    })?);
    let rows = builder.build().fetch_all(&mut *tx).await.map_err(SqliteStoreError::from)?;
    let records = rows
        .iter()
        .map(|row| materialize_row(row, query.filter.max_item_bytes))
        .collect::<Result<Vec<_>, _>>()?;
    tx.commit().await.map_err(SqliteStoreError::from)?;
    Ok(records)
}

async fn load_index(
    pool: &sqlx::SqlitePool,
    query: KnowledgeIndexQuery<'_>,
) -> Result<Vec<KnowledgeIndexRecord>, StoreError> {
    let mut tx = pool.begin().await.map_err(SqliteStoreError::from)?;
    verify_snapshot(&mut tx, query.snapshot).await?;
    let indexed_kinds = query
        .knowledge_kinds
        .iter()
        .copied()
        .filter(|kind| matches!(kind, KnowledgeKind::Fact | KnowledgeKind::Rumor))
        .collect::<Vec<_>>();
    if indexed_kinds.is_empty() || query.limit == 0 {
        tx.commit().await.map_err(SqliteStoreError::from)?;
        return Ok(Vec::new());
    }
    let mut builder = QueryBuilder::<Sqlite>::new(
        "SELECT source_id, knowledge_kind, retrieval_hint FROM knowledge_entries WHERE story_id = ",
    );
    builder.push_bind(query.snapshot.story_id.as_str());
    builder.push(" AND knowledge_kind IN (");
    {
        let mut separated = builder.separated(", ");
        for kind in indexed_kinds {
            separated.push_bind(kind_name(kind));
        }
    }
    builder.push(") ORDER BY source_id ASC LIMIT ");
    builder.push_bind(i64::try_from(query.limit).map_err(|_| StoreError::LimitExceeded {
        limit: "knowledge_index_limit",
    })?);
    let rows = builder.build().fetch_all(&mut *tx).await.map_err(SqliteStoreError::from)?;
    let mut records = Vec::with_capacity(rows.len());
    for row in rows {
        let source_id: String = row.try_get("source_id").map_err(SqliteStoreError::from)?;
        let kind_raw: String = row.try_get("knowledge_kind").map_err(SqliteStoreError::from)?;
        let retrieval_hint_raw: Option<String> = row.try_get("retrieval_hint").map_err(SqliteStoreError::from)?;
        let kind = parse_kind(&kind_raw)?;
        if !matches!(kind, KnowledgeKind::Fact | KnowledgeKind::Rumor) {
            return Err(invalid_record());
        }
        let retrieval_hint = retrieval_hint_raw
            .map(crate::domain::knowledge::RetrievalHint::try_new)
            .transpose()
            .map_err(|_| invalid_record())?
            .ok_or_else(invalid_record)?;
        records.push(KnowledgeIndexRecord {
            source_id: make_source_id(kind, source_id)?,
            retrieval_hint,
        });
    }
    tx.commit().await.map_err(SqliteStoreError::from)?;
    Ok(records)
}

fn validate_filter(filter: &KnowledgeFilter) -> Result<(), StoreError> {
    if filter.knowledge_kinds.is_empty() {
        return Err(StoreError::ConstraintViolation {
            constraint: "knowledge_kinds_empty".into(),
        });
    }
    if filter.max_item_bytes == 0 {
        return Err(StoreError::LimitExceeded {
            limit: "max_item_bytes",
        });
    }
    let includes_memory = filter.knowledge_kinds.contains(&KnowledgeKind::Memory);
    let includes_fact = filter.knowledge_kinds.contains(&KnowledgeKind::Fact);
    match &filter.delivery {
        KnowledgeDelivery::Writer if includes_memory => {
            return Err(StoreError::ConstraintViolation {
                constraint: "memory_forbidden_for_writer_delivery".into(),
            });
        }
        KnowledgeDelivery::Character { .. } if includes_fact => {
            return Err(StoreError::ConstraintViolation {
                constraint: "fact_forbidden_for_character_delivery".into(),
            });
        }
        _ => {}
    }
    Ok(())
}

fn push_authorization(builder: &mut QueryBuilder<'_, Sqlite>, filter: &KnowledgeFilter) {
    match &filter.delivery {
        KnowledgeDelivery::Writer => {}
        KnowledgeDelivery::Character { role_id } => {
            builder.push(" AND e.knowledge_kind != 'fact'");
            builder.push(" AND (e.knowledge_kind != 'memory' OR e.memory_owner_role_id = ");
            builder.push_bind(role_id.as_str().to_owned());
            builder.push(")");
        }
    }
}

async fn verify_snapshot(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    snapshot: &KnowledgeSnapshotRef,
) -> Result<(), StoreError> {
    let row: Option<(i64, String)> = sqlx::query_as(
        "SELECT s.revision, p.digest FROM stories s \
         INNER JOIN story_instances i ON i.story_id = s.id \
         INNER JOIN story_packs p ON p.pack_id = i.pack_id WHERE s.id = ?1",
    )
    .bind(snapshot.story_id.as_str())
    .fetch_optional(&mut **tx)
    .await
    .map_err(SqliteStoreError::from)?;
    let Some((revision, digest)) = row else {
        return Err(StoreError::NotFound);
    };
    let revision = u64::try_from(revision).map_err(|_| invalid_record())?;
    if revision != snapshot.base_revision.get() || digest != snapshot.pack_digest.to_string() {
        return Err(StoreError::RevisionConflict);
    }
    Ok(())
}

fn materialize_row(row: &sqlx::sqlite::SqliteRow, max_item_bytes: usize) -> Result<KnowledgeRecord, StoreError> {
    let source_id_raw: String = row.try_get("source_id").map_err(SqliteStoreError::from)?;
    let kind_raw: String = row.try_get("knowledge_kind").map_err(SqliteStoreError::from)?;
    let owner_raw: Option<String> = row.try_get("memory_owner_role_id").map_err(SqliteStoreError::from)?;
    let content_raw: String = row.try_get("content").map_err(SqliteStoreError::from)?;
    let content_bytes: i64 = row.try_get("content_bytes").map_err(SqliteStoreError::from)?;
    let salience: i64 = row.try_get("salience").map_err(SqliteStoreError::from)?;
    let source_json: String = row.try_get("source_json").map_err(SqliteStoreError::from)?;
    let payload_json: String = row.try_get("payload_json").map_err(SqliteStoreError::from)?;
    let payload_bytes: i64 = row.try_get("payload_bytes").map_err(SqliteStoreError::from)?;
    let content_bytes = usize::try_from(content_bytes).map_err(|_| invalid_record())?;
    if content_bytes > max_item_bytes {
        return Err(StoreError::LimitExceeded {
            limit: "max_item_bytes",
        });
    }
    let payload_limit =
        max_item_bytes
            .checked_mul(4)
            .and_then(|value| value.checked_add(4096))
            .ok_or(StoreError::LimitExceeded {
                limit: "knowledge_payload_bytes",
            })?;
    if usize::try_from(payload_bytes).map_err(|_| invalid_record())? > payload_limit {
        return Err(StoreError::LimitExceeded {
            limit: "knowledge_payload_bytes",
        });
    }
    let kind = parse_kind(&kind_raw)?;
    let memory_owner = owner_raw.map(RoleId::try_new).transpose().map_err(|_| invalid_record())?;
    if (kind == KnowledgeKind::Memory) != memory_owner.is_some() {
        return Err(invalid_record());
    }
    let source_id = make_source_id(kind, source_id_raw)?;
    let source = serde_json::from_str::<KnowledgeSource>(&source_json).map_err(|_| invalid_record())?;
    let content =
        BoundedText::try_new(content_raw, "knowledge_content", max_item_bytes).map_err(|_| invalid_record())?;
    let payload = serde_json::from_str::<crate::domain::knowledge::KnowledgeEntry>(&payload_json)
        .map_err(|_| invalid_record())?;
    let (activation, activation_rule_version) = match &payload {
        crate::domain::knowledge::KnowledgeEntry::Fact(value) => {
            (Some(value.activation.clone()), Some(value.activation_rule_version.clone()))
        }
        crate::domain::knowledge::KnowledgeEntry::Rumor(value) => {
            (Some(value.activation.clone()), Some(value.activation_rule_version.clone()))
        }
        crate::domain::knowledge::KnowledgeEntry::Memory(_) => (None, None),
    };
    let record = KnowledgeRecord {
        source_id,
        kind,
        content,
        salience: u8::try_from(salience).map_err(|_| invalid_record())?,
        source,
        memory_owner,
        activation,
        activation_rule_version,
    };
    if payload.source_id() != record.source_id
        || payload.kind() != record.kind
        || payload.content().as_str() != record.content.as_str()
        || payload.salience() != record.salience
        || payload.source() != &record.source
        || payload.memory_owner() != record.memory_owner.as_ref()
    {
        return Err(invalid_record());
    }
    Ok(record)
}

fn parse_kind(value: &str) -> Result<KnowledgeKind, StoreError> {
    match value {
        "fact" => Ok(KnowledgeKind::Fact),
        "rumor" => Ok(KnowledgeKind::Rumor),
        "memory" => Ok(KnowledgeKind::Memory),
        _ => Err(invalid_record()),
    }
}

fn kind_name(kind: KnowledgeKind) -> &'static str {
    match kind {
        KnowledgeKind::Fact => "fact",
        KnowledgeKind::Rumor => "rumor",
        KnowledgeKind::Memory => "memory",
    }
}

fn source_id_kind(source_id: &KnowledgeSourceId) -> KnowledgeKind {
    match source_id {
        KnowledgeSourceId::Fact(_) => KnowledgeKind::Fact,
        KnowledgeSourceId::Rumor(_) => KnowledgeKind::Rumor,
        KnowledgeSourceId::Memory(_) => KnowledgeKind::Memory,
    }
}

fn make_source_id(kind: KnowledgeKind, value: String) -> Result<KnowledgeSourceId, StoreError> {
    KnowledgeSourceId::try_from_parts(kind, &value).map_err(|_| invalid_record())
}

fn invalid_record() -> StoreError {
    StoreError::Serialization {
        kind: StoreSerializationErrorKind::InvalidStoryState,
    }
}

impl SqliteStore {
    pub(crate) fn pool(&self) -> &sqlx::SqlitePool {
        self.pool_for_tests()
    }
}
