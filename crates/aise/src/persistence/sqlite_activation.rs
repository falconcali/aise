use crate::config::ActivationIndexLimits;
use crate::domain::asset::ids::Sha256Digest;
use crate::domain::knowledge::activation::{
    ActivationEntryMetadata, ActivationIndexSnapshot, ActivationIndexSnapshotRef, ActivationRuleVersion,
    ActivationTimedState,
};
use crate::domain::knowledge::KnowledgeEntry;
use crate::domain::story_instance::snapshot::KnowledgeSnapshotRef;
use crate::persistence::activation_index_port::ActivationIndexPort;
use crate::persistence::activation_timed_state_port::{
    ActivationTimedStateQuery, ActivationTimedStateReadPort,
};
use crate::persistence::sqlite_error::SqliteStoreError;
use crate::persistence::sqlite_store::SqliteStore;
use crate::persistence::store::{StoreError, StoreSerializationErrorKind};
use async_trait::async_trait;
use sqlx::Row;
use std::collections::BTreeMap;
use std::sync::Arc;

const MATCHER_VERSION: u32 = 1;

#[async_trait]
impl ActivationIndexPort for SqliteStore {
    async fn load_snapshot(
        &self,
        knowledge: &KnowledgeSnapshotRef,
        limits: ActivationIndexLimits,
    ) -> Result<Arc<ActivationIndexSnapshot>, StoreError> {
        let mut tx = self.pool().begin().await.map_err(SqliteStoreError::from)?;
        let overlay_version: i64 = sqlx::query_scalar(
            "SELECT activation_overlay_version FROM story_instances WHERE story_id = ?1",
        )
        .bind(knowledge.story_id.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(SqliteStoreError::from)?;
        let rows = sqlx::query(
            "SELECT payload_json FROM knowledge_entries WHERE story_id = ?1 AND knowledge_kind != 'memory'
             ORDER BY source_id LIMIT ?2",
        )
        .bind(knowledge.story_id.as_str())
        .bind(i64::try_from(limits.max_entries).map_err(|_| StoreError::LimitExceeded {
            limit: "activation_index_entries",
        })?)
        .fetch_all(&mut *tx)
        .await
        .map_err(SqliteStoreError::from)?;
        let mut metadata = BTreeMap::new();
        for row in rows {
            let payload: String = row.try_get("payload_json").map_err(SqliteStoreError::from)?;
            let entry: KnowledgeEntry =
                serde_json::from_str(&payload).map_err(|_| StoreError::Serialization {
                    kind: StoreSerializationErrorKind::InvalidWorldState,
                })?;
            let metadata_entry = match &entry {
                KnowledgeEntry::Fact(value) => ActivationEntryMetadata {
                    source_id: entry.source_id(),
                    kind: entry.kind(),
                    rule: value.activation.clone(),
                    rule_version: value.activation_rule_version.clone(),
                    salience: value.salience,
                },
                KnowledgeEntry::Rumor(value) => ActivationEntryMetadata {
                    source_id: entry.source_id(),
                    kind: entry.kind(),
                    rule: value.activation.clone(),
                    rule_version: value.activation_rule_version.clone(),
                    salience: value.salience,
                },
                KnowledgeEntry::Memory(_) => continue,
            };
            metadata.insert(metadata_entry.source_id.clone(), metadata_entry);
        }
        tx.commit().await.map_err(SqliteStoreError::from)?;
        let overlay_version = u64::try_from(overlay_version).map_err(|_| StoreError::Serialization {
            kind: StoreSerializationErrorKind::InvalidWorldState,
        })?;
        let reference = ActivationIndexSnapshotRef::from_knowledge(knowledge, overlay_version, MATCHER_VERSION);
        Ok(Arc::new(ActivationIndexSnapshot::new(reference, metadata)))
    }
}

#[async_trait]
impl ActivationIndexPort for Arc<SqliteStore> {
    async fn load_snapshot(
        &self,
        knowledge: &KnowledgeSnapshotRef,
        limits: ActivationIndexLimits,
    ) -> Result<Arc<ActivationIndexSnapshot>, StoreError> {
        ActivationIndexPort::load_snapshot(&**self, knowledge, limits).await
    }
}

#[async_trait]
impl ActivationTimedStateReadPort for SqliteStore {
    async fn load_timed_state(
        &self,
        query: ActivationTimedStateQuery<'_>,
    ) -> Result<Vec<ActivationTimedState>, StoreError> {
        if query.limit == 0 {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(
            "SELECT source_id, rule_version, sticky_through_turn, cooldown_through_turn
             FROM knowledge_activation_timed_state WHERE story_id = ?1 ORDER BY source_id LIMIT ?2",
        )
        .bind(query.snapshot.story_id.as_str())
        .bind(i64::try_from(query.limit).map_err(|_| StoreError::LimitExceeded {
            limit: "activation_timed_state",
        })?)
        .fetch_all(self.pool())
        .await
        .map_err(SqliteStoreError::from)?;
        rows.into_iter()
            .map(|row| {
                let source_id: String = row.try_get("source_id").map_err(SqliteStoreError::from)?;
                let rule_version: String = row.try_get("rule_version").map_err(SqliteStoreError::from)?;
                let source_id = serde_json::from_value(serde_json::json!(source_id)).map_err(|_| StoreError::Serialization {
                    kind: StoreSerializationErrorKind::InvalidWorldState,
                })?;
                let rule_version = ActivationRuleVersion(
                    Sha256Digest::try_new(&rule_version).map_err(|_| StoreError::Serialization {
                        kind: StoreSerializationErrorKind::InvalidWorldState,
                    })?,
                );
                Ok(ActivationTimedState {
                    source_id,
                    rule_version,
                    sticky_through_turn: row
                        .try_get::<Option<i64>, _>("sticky_through_turn")
                        .map_err(SqliteStoreError::from)?
                        .map(|value| value as u64)
                        .and_then(|value| crate::domain::ids::TurnNumber::try_new(value).ok()),
                    cooldown_through_turn: row
                        .try_get::<Option<i64>, _>("cooldown_through_turn")
                        .map_err(SqliteStoreError::from)?
                        .map(|value| value as u64)
                        .and_then(|value| crate::domain::ids::TurnNumber::try_new(value).ok()),
                })
            })
            .collect()
    }
}

#[async_trait]
impl ActivationTimedStateReadPort for Arc<SqliteStore> {
    async fn load_timed_state(
        &self,
        query: ActivationTimedStateQuery<'_>,
    ) -> Result<Vec<ActivationTimedState>, StoreError> {
        ActivationTimedStateReadPort::load_timed_state(&**self, query).await
    }
}
