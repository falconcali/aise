use crate::domain::asset::ids::Sha256Digest;
use crate::domain::knowledge::activation::{
    ActivationEntryMetadata, ActivationIndexLimits, ActivationIndexMetadata, ActivationIndexSnapshotRef,
    ActivationRuleVersion, ActivationTimedState, MATCHER_VERSION,
};
use crate::domain::knowledge::{
    KnowledgeIdHighWater, KnowledgeKind, KnowledgeSource, KnowledgeSourceId, allocate_knowledge_ids,
};
use crate::domain::story_instance::snapshot::KnowledgeSnapshotRef;
use crate::persistence::activation_index_port::ActivationIndexPort;
use crate::persistence::activation_timed_state_port::{ActivationTimedStateQuery, ActivationTimedStateReadPort};
use crate::persistence::sqlite_error::SqliteStoreError;
use crate::persistence::sqlite_store::SqliteStore;
use crate::persistence::store::{StoreError, StoreSerializationErrorKind};
use async_trait::async_trait;
use sqlx::Row;
use std::collections::BTreeMap;
use std::sync::Arc;

#[async_trait]
impl ActivationIndexPort for SqliteStore {
    async fn load_snapshot(
        &self,
        knowledge: &KnowledgeSnapshotRef,
        limits: ActivationIndexLimits,
    ) -> Result<Arc<ActivationIndexMetadata>, StoreError> {
        let mut tx = self.pool().begin().await.map_err(SqliteStoreError::from)?;
        let snapshot_row: Option<(i64, String, i64)> = sqlx::query_as(
            "SELECT s.revision, p.digest, i.activation_overlay_version
             FROM stories s
             INNER JOIN story_instances i ON i.story_id = s.id
             INNER JOIN story_packs p ON p.pack_id = i.pack_id
             WHERE s.id = ?1",
        )
        .bind(knowledge.story_id.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(SqliteStoreError::from)?;
        let Some((revision, digest, overlay_version)) = snapshot_row else {
            return Err(StoreError::NotFound);
        };
        let revision = u64::try_from(revision).map_err(|_| invalid_activation_state())?;
        if revision != knowledge.base_revision.get() || digest != knowledge.pack_digest.to_string() {
            return Err(StoreError::RevisionConflict);
        }
        let pack_entries = canonical_pack_entries(&mut tx, knowledge, limits.max_entries).await?;
        let entry_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM knowledge_entries WHERE story_id = ?1 AND knowledge_kind != 'memory'",
        )
        .bind(knowledge.story_id.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(SqliteStoreError::from)?;
        if entry_count < 0 || usize::try_from(entry_count).ok().is_none_or(|count| count > limits.max_entries) {
            return Err(StoreError::LimitExceeded { limit: "max_entries" });
        }
        let rows = sqlx::query(
            "SELECT source_id, knowledge_kind, salience, source_json, activation_rule_json, activation_rule_version
             FROM knowledge_entries WHERE story_id = ?1 AND knowledge_kind != 'memory' ORDER BY source_id",
        )
        .bind(knowledge.story_id.as_str())
        .fetch_all(&mut *tx)
        .await
        .map_err(SqliteStoreError::from)?;
        let mut entries = BTreeMap::new();
        let mut overlay_entries = 0usize;
        for row in rows {
            let source_id: String = row.try_get("source_id").map_err(SqliteStoreError::from)?;
            let kind = match row
                .try_get::<String, _>("knowledge_kind")
                .map_err(SqliteStoreError::from)?
                .as_str()
            {
                "fact" => KnowledgeKind::Fact,
                "rumor" => KnowledgeKind::Rumor,
                _ => {
                    return Err(StoreError::Serialization {
                        kind: StoreSerializationErrorKind::InvalidWorldState,
                    });
                }
            };
            let source_id =
                KnowledgeSourceId::try_from_parts(kind, &source_id).map_err(|_| StoreError::Serialization {
                    kind: StoreSerializationErrorKind::InvalidWorldState,
                })?;
            let source: KnowledgeSource =
                serde_json::from_str(&row.try_get::<String, _>("source_json").map_err(SqliteStoreError::from)?)
                    .map_err(|_| StoreError::Serialization {
                        kind: StoreSerializationErrorKind::InvalidWorldState,
                    })?;
            let rule = serde_json::from_str(
                &row.try_get::<String, _>("activation_rule_json")
                    .map_err(SqliteStoreError::from)?,
            )
            .map_err(|_| StoreError::Serialization {
                kind: StoreSerializationErrorKind::InvalidWorldState,
            })?;
            let rule_version = ActivationRuleVersion::from_digest(
                Sha256Digest::try_new(
                    &row.try_get::<String, _>("activation_rule_version")
                        .map_err(SqliteStoreError::from)?,
                )
                .map_err(|_| StoreError::Serialization {
                    kind: StoreSerializationErrorKind::InvalidWorldState,
                })?,
            );
            let salience =
                u8::try_from(row.try_get::<i64, _>("salience").map_err(SqliteStoreError::from)?).map_err(|_| {
                    StoreError::Serialization {
                        kind: StoreSerializationErrorKind::InvalidWorldState,
                    }
                })?;
            let from_pack = matches!(
                source,
                KnowledgeSource::Seed { pack_digest, .. } if pack_digest == knowledge.pack_digest
            );
            if !from_pack {
                overlay_entries = overlay_entries.saturating_add(1);
                if overlay_entries > limits.max_overlay_entries {
                    return Err(StoreError::LimitExceeded {
                        limit: "max_overlay_entries",
                    });
                }
            }
            let metadata_entry = ActivationEntryMetadata {
                source_id: source_id.clone(),
                kind,
                rule,
                rule_version,
                salience,
                from_pack,
            };
            entries.insert(source_id, metadata_entry);
        }
        tx.commit().await.map_err(SqliteStoreError::from)?;
        let overlay_version = u64::try_from(overlay_version).map_err(|_| StoreError::Serialization {
            kind: StoreSerializationErrorKind::InvalidWorldState,
        })?;
        let reference = ActivationIndexSnapshotRef::from_knowledge(knowledge, overlay_version, MATCHER_VERSION);
        Ok(Arc::new(ActivationIndexMetadata {
            reference,
            pack_entries,
            entries,
        }))
    }
}

async fn canonical_pack_entries(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    knowledge: &KnowledgeSnapshotRef,
    max_entries: usize,
) -> Result<BTreeMap<KnowledgeSourceId, ActivationEntryMetadata>, StoreError> {
    let fetch_limit = max_entries
        .checked_add(1)
        .ok_or(StoreError::LimitExceeded { limit: "max_entries" })?;
    let rows = sqlx::query(
        "SELECT knowledge_kind, entry_key, salience, activation_rule_json FROM (
             SELECT 0 AS kind_order, 'fact' AS knowledge_kind, entry.key AS entry_key,
                    CAST(json_extract(entry.value, '$.salience') AS INTEGER) AS salience,
                    json_extract(entry.value, '$.activation') AS activation_rule_json
             FROM story_instances i
             INNER JOIN story_packs p ON p.pack_id = i.pack_id
             INNER JOIN json_each(p.world_book_json, '$.facts') entry
             WHERE i.story_id = ?1
             UNION ALL
             SELECT 1 AS kind_order, 'rumor' AS knowledge_kind, entry.key AS entry_key,
                    CAST(json_extract(entry.value, '$.salience') AS INTEGER) AS salience,
                    json_extract(entry.value, '$.activation') AS activation_rule_json
             FROM story_instances i
             INNER JOIN story_packs p ON p.pack_id = i.pack_id
             INNER JOIN json_each(p.world_book_json, '$.rumors') entry
             WHERE i.story_id = ?1
         ) ORDER BY kind_order, entry_key LIMIT ?2",
    )
    .bind(knowledge.story_id.as_str())
    .bind(i64::try_from(fetch_limit).map_err(|_| StoreError::LimitExceeded { limit: "max_entries" })?)
    .fetch_all(&mut **tx)
    .await
    .map_err(SqliteStoreError::from)?;
    if rows.len() > max_entries {
        return Err(StoreError::LimitExceeded { limit: "max_entries" });
    }
    let kinds = rows
        .iter()
        .map(|row| {
            let kind = row.try_get::<String, _>("knowledge_kind").map_err(SqliteStoreError::from)?;
            match kind.as_str() {
                "fact" => Ok(KnowledgeKind::Fact),
                "rumor" => Ok(KnowledgeKind::Rumor),
                _ => Err(invalid_activation_state()),
            }
        })
        .collect::<Result<Vec<_>, StoreError>>()?;
    let allocation =
        allocate_knowledge_ids(KnowledgeIdHighWater::zero(), &kinds).map_err(|_| invalid_activation_state())?;
    let mut assigned = allocation.assigned.into_iter();
    let mut entries = BTreeMap::new();
    for row in rows {
        let source_id = assigned.next().ok_or_else(invalid_activation_state)?;
        let kind = source_id.kind();
        let rule: crate::domain::knowledge::activation::KnowledgeActivationRule = serde_json::from_str(
            &row.try_get::<String, _>("activation_rule_json")
                .map_err(SqliteStoreError::from)?,
        )
        .map_err(|_| invalid_activation_state())?;
        let salience = u8::try_from(row.try_get::<i64, _>("salience").map_err(SqliteStoreError::from)?)
            .map_err(|_| invalid_activation_state())?;
        let rule_version = ActivationRuleVersion::from_rule(&rule);
        entries.insert(
            source_id.clone(),
            ActivationEntryMetadata {
                source_id,
                kind,
                rule,
                rule_version,
                salience,
                from_pack: true,
            },
        );
    }
    Ok(entries)
}

fn invalid_activation_state() -> StoreError {
    StoreError::Serialization {
        kind: StoreSerializationErrorKind::InvalidWorldState,
    }
}

#[async_trait]
impl ActivationIndexPort for Arc<SqliteStore> {
    async fn load_snapshot(
        &self,
        knowledge: &KnowledgeSnapshotRef,
        limits: ActivationIndexLimits,
    ) -> Result<Arc<ActivationIndexMetadata>, StoreError> {
        ActivationIndexPort::load_snapshot(&**self, knowledge, limits).await
    }
}

async fn verify_activation_snapshot(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    snapshot: &KnowledgeSnapshotRef,
) -> Result<(), StoreError> {
    let row: Option<(i64, String)> = sqlx::query_as(
        "SELECT s.revision, p.digest FROM stories s
         INNER JOIN story_instances i ON i.story_id = s.id
         INNER JOIN story_packs p ON p.pack_id = i.pack_id
         WHERE s.id = ?1",
    )
    .bind(snapshot.story_id.as_str())
    .fetch_optional(&mut **tx)
    .await
    .map_err(SqliteStoreError::from)?;
    let Some((revision, digest)) = row else {
        return Err(StoreError::NotFound);
    };
    let revision = u64::try_from(revision).map_err(|_| invalid_activation_state())?;
    if revision != snapshot.base_revision.get() || digest != snapshot.pack_digest.to_string() {
        return Err(StoreError::RevisionConflict);
    }
    Ok(())
}

#[async_trait]
impl ActivationTimedStateReadPort for SqliteStore {
    async fn load_timed_state(
        &self,
        query: ActivationTimedStateQuery<'_>,
    ) -> Result<Vec<ActivationTimedState>, StoreError> {
        if query.limit == 0 {
            return Err(StoreError::LimitExceeded {
                limit: "activation_timed_state",
            });
        }
        let mut tx = self.pool().begin().await.map_err(SqliteStoreError::from)?;
        verify_activation_snapshot(&mut tx, query.snapshot).await?;
        let fetch_limit = query.limit.checked_add(1).ok_or(StoreError::LimitExceeded {
            limit: "activation_timed_state",
        })?;
        let rows = sqlx::query(
            "SELECT source_id, rule_version, sticky_through_turn, cooldown_through_turn
             FROM knowledge_activation_timed_state WHERE story_id = ?1 ORDER BY source_id LIMIT ?2",
        )
        .bind(query.snapshot.story_id.as_str())
        .bind(i64::try_from(fetch_limit).map_err(|_| StoreError::LimitExceeded {
            limit: "activation_timed_state",
        })?)
        .fetch_all(&mut *tx)
        .await
        .map_err(SqliteStoreError::from)?;
        if rows.len() > query.limit {
            return Err(StoreError::LimitExceeded {
                limit: "activation_timed_state",
            });
        }
        let states = rows
            .into_iter()
            .map(|row| {
                let source_id: String = row.try_get("source_id").map_err(SqliteStoreError::from)?;
                let rule_version: String = row.try_get("rule_version").map_err(SqliteStoreError::from)?;
                let kind = if source_id.starts_with("fact_") {
                    KnowledgeKind::Fact
                } else if source_id.starts_with("rumor_") {
                    KnowledgeKind::Rumor
                } else if source_id.starts_with("memory_") {
                    KnowledgeKind::Memory
                } else {
                    return Err(StoreError::Serialization {
                        kind: StoreSerializationErrorKind::InvalidWorldState,
                    });
                };
                let source_id =
                    KnowledgeSourceId::try_from_parts(kind, &source_id).map_err(|_| StoreError::Serialization {
                        kind: StoreSerializationErrorKind::InvalidWorldState,
                    })?;
                let rule_version =
                    ActivationRuleVersion::from_digest(Sha256Digest::try_new(&rule_version).map_err(|_| {
                        StoreError::Serialization {
                            kind: StoreSerializationErrorKind::InvalidWorldState,
                        }
                    })?);
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
            .collect::<Result<Vec<_>, StoreError>>()?;
        tx.commit().await.map_err(SqliteStoreError::from)?;
        Ok(states)
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
