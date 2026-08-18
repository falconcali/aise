use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::frozen_ref::FrozenStoryPackRef;
use crate::domain::asset::ids::{FactKey, PackId, Sha256Digest, TopicKey};
use crate::domain::asset::story_pack::StoryProfile;
use crate::domain::asset::validation::{BoundedText, ScalarValue};
use crate::domain::asset::world_book::TopicDefinition;
use crate::domain::ids::{RoleId, StoryId, StoryRevision, TurnNumber};
use crate::domain::narrative::{StoryContinuity, StorySegment, StorySegmentOrigin, StorySummary};
use crate::domain::narrative_graph::definition::NarrativeGraphDefinition;
use crate::domain::narrative_graph::state::NarrativeRuntimeState;
use crate::domain::story_instance::constraint::ActiveStoryConstraint;
use crate::domain::story_instance::role::{StoryRole, StoryRoleView};
use crate::domain::story_instance::snapshot::{KnowledgeSnapshotRef, StoryReadSnapshot, StoryReadSnapshotParts};
use crate::domain::story_instance::state::{InstanceSettings, RelationshipState};
use crate::domain::turn::SnapshotLimits;
use crate::persistence::sqlite_error::SqliteStoreError;
use crate::persistence::store::StoreError;
use sqlx::SqlitePool;
use std::collections::BTreeMap;

type StoryInstanceRow = (i64, String, String, String, String, String, String, String, String, i64, i64);
type StoryPackRow = (String, String, String, Vec<u8>, Vec<u8>, Vec<u8>, String);
type InstanceProjectionLengths = (String, i64, i64, i64, i64, i64, i64, i64);
type PackProjectionLengths = (i64, i64, i64, Option<i64>);

pub(crate) async fn load_story_snapshot(
    pool: &SqlitePool,
    story_id: &StoryId,
    limits: SnapshotLimits,
) -> Result<StoryReadSnapshot, StoreError> {
    let mut tx = pool.begin().await.map_err(SqliteStoreError::from)?;
    let instance_lengths: Option<InstanceProjectionLengths> = sqlx::query_as(
        "SELECT i.pack_id, length(i.settings_json), length(i.roles_json), \
                length(i.relationships_json), length(i.narrative_state_json), \
                length(i.fact_values_json), length(s.story_summary), \
                length(s.active_constraints) \
         FROM stories s INNER JOIN story_instances i ON i.story_id = s.id WHERE s.id = ?",
    )
    .bind(story_id.as_str())
    .fetch_optional(&mut *tx)
    .await
    .map_err(SqliteStoreError::from)?;
    let Some((
        projection_pack_id,
        settings_len,
        roles_len,
        relationships_len,
        narrative_state_len,
        fact_values_len,
        summary_len,
        constraints_len,
    )) = instance_lengths
    else {
        return Err(StoreError::NotFound);
    };
    ensure_projection_length(
        settings_len,
        projection_limit(limits.max_instance_settings, limits.max_instance_setting_bytes, 1024)?,
        "settings_json",
    )?;
    ensure_projection_length(
        roles_len,
        projection_limit(limits.max_roles, limits.max_role_bytes, 1024)?,
        "roles_json",
    )?;
    ensure_projection_length(
        relationships_len,
        projection_limit(limits.max_relationships, limits.max_role_bytes, 1024)?,
        "relationships_json",
    )?;
    ensure_projection_length(
        narrative_state_len,
        projection_limit(limits.max_narrative_nodes, limits.max_role_bytes, 1024)?,
        "narrative_state_json",
    )?;
    ensure_projection_length(
        fact_values_len,
        projection_limit(limits.max_condition_fact_values, limits.max_constraint_bytes, 1024)?,
        "fact_values_json",
    )?;
    ensure_projection_length(
        summary_len,
        limits
            .continuity
            .max_summary_bytes
            .checked_add(256)
            .ok_or(StoreError::LimitExceeded { limit: "story_summary" })?,
        "story_summary",
    )?;
    ensure_projection_length(
        constraints_len,
        projection_limit(limits.max_constraints, limits.max_constraint_bytes, 1024)?,
        "active_constraints",
    )?;
    let pack_lengths: Option<PackProjectionLengths> = sqlx::query_as(
        "SELECT length(story_profile_json), length(narrative_definition_json), length(topic_dictionary_json), \
                length(json_extract(pack_json, '$.meta.title')) \
         FROM story_packs WHERE pack_id = ?",
    )
    .bind(&projection_pack_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(SqliteStoreError::from)?;
    let Some((profile_len, narrative_len, topics_len, title_len)) = pack_lengths else {
        return Err(StoreError::NotFound);
    };
    ensure_projection_length(profile_len, limits.max_story_profile_bytes, "story_profile_json")?;
    let title_len = title_len.ok_or(StoreError::Serialization {
        kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
    })?;
    ensure_projection_length(title_len, limits.max_story_profile_bytes, "story_title")?;
    ensure_projection_length(
        narrative_len,
        projection_limit(limits.max_narrative_nodes, limits.max_role_bytes, 1024)?,
        "narrative_definition_json",
    )?;
    let topic_items = limits
        .max_topics
        .checked_mul(
            limits
                .max_topic_aliases_per_topic
                .checked_add(1)
                .ok_or(StoreError::LimitExceeded {
                    limit: "topic_dictionary_json",
                })?,
        )
        .ok_or(StoreError::LimitExceeded {
            limit: "topic_dictionary_json",
        })?;
    ensure_projection_length(
        topics_len,
        projection_limit(topic_items, limits.max_role_bytes, 1024)?,
        "topic_dictionary_json",
    )?;
    let row: Option<StoryInstanceRow> = sqlx::query_as(
        "SELECT s.revision, i.pack_id, i.settings_json, i.roles_json, \
                i.relationships_json, i.narrative_state_json, \
                i.fact_values_json, s.story_summary, s.active_constraints, \
                i.knowledge_id_high_water, i.role_id_high_water \
         FROM stories s \
         INNER JOIN story_instances i ON i.story_id = s.id \
         WHERE s.id = ?",
    )
    .bind(story_id.as_str())
    .fetch_optional(&mut *tx)
    .await
    .map_err(SqliteStoreError::from)?;
    let Some((
        revision,
        pack_id,
        settings_json,
        roles_json,
        relationships_json,
        narrative_state_json,
        fact_values_json,
        story_summary_json,
        active_constraints_json,
        knowledge_id_high_water,
        role_id_high_water,
    )) = row
    else {
        tx.rollback().await.map_err(SqliteStoreError::from)?;
        return Err(StoreError::NotFound);
    };
    let pack_row: Option<StoryPackRow> = sqlx::query_as(
        "SELECT pack_key, version, digest, story_profile_json, narrative_definition_json, topic_dictionary_json, \
                json_extract(pack_json, '$.meta.title') \
         FROM story_packs WHERE pack_id = ?",
    )
    .bind(&pack_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(SqliteStoreError::from)?;
    let Some((
        pack_key,
        pack_version,
        digest_raw,
        story_profile_json,
        narrative_definition_json,
        topic_dictionary_json,
        story_title_raw,
    )) = pack_row
    else {
        tx.rollback().await.map_err(SqliteStoreError::from)?;
        return Err(StoreError::NotFound);
    };
    let story_title =
        BoundedText::try_new(story_title_raw, "story_title", limits.max_story_profile_bytes).map_err(|_| {
            StoreError::Serialization {
                kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
            }
        })?;
    if story_title.as_str().trim().is_empty() {
        return Err(StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
        });
    }
    let story_profile: StoryProfile =
        serde_json::from_slice(&story_profile_json).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
        })?;
    let digest = Sha256Digest::try_new(&digest_raw).map_err(|_| StoreError::Serialization {
        kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
    })?;
    let narrative_definition: NarrativeGraphDefinition =
        serde_json::from_slice(&narrative_definition_json).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
        })?;
    let topic_dictionary: BTreeMap<TopicKey, TopicDefinition> = serde_json::from_slice(&topic_dictionary_json)
        .map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidWorldState,
        })?;
    let instance_settings: InstanceSettings =
        serde_json::from_str(&settings_json).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
        })?;
    let roles: BTreeMap<RoleId, StoryRole> =
        serde_json::from_str(&roles_json).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidRoleState,
        })?;
    if roles.len() > limits.max_roles {
        return Err(StoreError::ConstraintViolation {
            constraint: "max_roles".into(),
        });
    }
    for role in roles.values() {
        let role_bytes = role.compact_byte_len().map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidRoleState,
        })?;
        if role_bytes > limits.max_role_bytes {
            return Err(StoreError::ConstraintViolation {
                constraint: "max_role_bytes".into(),
            });
        }
    }
    let roles: BTreeMap<RoleId, StoryRoleView> =
        roles.into_iter().map(|(id, role)| (id, StoryRoleView::from(&role))).collect();
    let relationships: Vec<RelationshipState> =
        serde_json::from_str(&relationships_json).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
        })?;
    if relationships.len() > limits.max_relationships {
        return Err(StoreError::ConstraintViolation {
            constraint: "max_relationships".into(),
        });
    }
    let narrative_state: NarrativeRuntimeState =
        serde_json::from_str(&narrative_state_json).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
        })?;
    let fact_values: BTreeMap<FactKey, ScalarValue> =
        serde_json::from_str(&fact_values_json).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
        })?;
    let summary: StorySummary = serde_json::from_str(&story_summary_json).map_err(|_| StoreError::Serialization {
        kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
    })?;
    let active_constraints: Vec<ActiveStoryConstraint> =
        serde_json::from_str(&active_constraints_json).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
        })?;
    if active_constraints.len() > limits.max_constraints {
        return Err(StoreError::ConstraintViolation {
            constraint: "max_constraints".into(),
        });
    }
    let summary_through = summary.summarized_through.map(|sequence| sequence.get()).unwrap_or(0);
    let segment_limit = limits
        .continuity
        .max_recent_segments
        .checked_add(1)
        .ok_or(StoreError::LimitExceeded {
            limit: "max_recent_segments",
        })?;
    let segment_rows: Vec<(String, Option<i64>, i64, String)> = sqlx::query_as(
        "SELECT origin, turn_number, sequence, story_text FROM story_segments \
         WHERE story_id = ? AND sequence > ? \
         ORDER BY sequence ASC LIMIT ?",
    )
    .bind(story_id.as_str())
    .bind(i64::try_from(summary_through).map_err(|_| StoreError::Serialization {
        kind: crate::persistence::store::StoreSerializationErrorKind::InvalidTurnResult,
    })?)
    .bind(i64::try_from(segment_limit).map_err(|_| StoreError::LimitExceeded {
        limit: "max_recent_segments",
    })?)
    .fetch_all(&mut *tx)
    .await
    .map_err(SqliteStoreError::from)?;
    if segment_rows.len() > limits.continuity.max_recent_segments {
        return Err(StoreError::LimitExceeded {
            limit: "max_recent_segments",
        });
    }
    let mut recent_segments = Vec::new();
    for (origin, turn_number, sequence, story_text) in segment_rows {
        let origin = match (origin.as_str(), turn_number) {
            ("opening", None) => StorySegmentOrigin::Opening,
            ("turn", Some(turn_number)) => StorySegmentOrigin::Turn {
                turn_number: TurnNumber::try_new(turn_number as u64).map_err(|_| StoreError::Serialization {
                    kind: crate::persistence::store::StoreSerializationErrorKind::InvalidTurnResult,
                })?,
            },
            _ => {
                return Err(StoreError::Serialization {
                    kind: crate::persistence::store::StoreSerializationErrorKind::InvalidTurnResult,
                });
            }
        };
        let sequence =
            crate::domain::StorySequence::try_new(sequence as u64).map_err(|_| StoreError::Serialization {
                kind: crate::persistence::store::StoreSerializationErrorKind::InvalidTurnResult,
            })?;
        let text = BoundedText::try_new(story_text, "recent_segment", limits.continuity.max_recent_segment_bytes)
            .map_err(|_| StoreError::ConstraintViolation {
                constraint: "max_recent_segment_bytes".into(),
            })?;
        recent_segments.push(StorySegment { sequence, origin, text });
    }
    let story_continuity = StoryContinuity::try_new(summary, recent_segments, limits.continuity).map_err(|_| {
        StoreError::ConstraintViolation {
            constraint: "story_continuity".into(),
        }
    })?;
    let entity_limit = limits.max_entity_catalog.checked_add(1).ok_or(StoreError::LimitExceeded {
        limit: "max_entity_catalog",
    })?;
    let entity_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT DISTINCT entity_kind, entity_key FROM knowledge_entry_entities \
         WHERE story_id = ? ORDER BY entity_kind ASC, entity_key ASC LIMIT ?",
    )
    .bind(story_id.as_str())
    .bind(i64::try_from(entity_limit).map_err(|_| StoreError::LimitExceeded {
        limit: "max_entity_catalog",
    })?)
    .fetch_all(&mut *tx)
    .await
    .map_err(SqliteStoreError::from)?;
    if entity_rows.len() > limits.max_entity_catalog {
        return Err(StoreError::LimitExceeded {
            limit: "max_entity_catalog",
        });
    }
    let mut entity_catalog = Vec::new();
    for (kind, key) in entity_rows {
        entity_catalog.push(parse_entity(&kind, &key)?);
    }
    entity_catalog.sort();
    entity_catalog.dedup();
    if topic_dictionary.len() > limits.max_topics {
        return Err(StoreError::ConstraintViolation {
            constraint: "max_topics".into(),
        });
    }
    for definition in topic_dictionary.values() {
        if definition.aliases.len() > limits.max_topic_aliases_per_topic {
            return Err(StoreError::ConstraintViolation {
                constraint: "max_topic_aliases_per_topic".into(),
            });
        }
    }
    let pack_ref = FrozenStoryPackRef {
        pack_id: PackId::from(pack_id),
        pack_key: crate::domain::asset::ids::StoryPackKey::from(pack_key),
        version: crate::domain::asset::ids::SemanticVersion::try_new(pack_version).map_err(|_| {
            StoreError::Serialization {
                kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
            }
        })?,
        digest: digest.clone(),
    };
    let base_revision = StoryRevision::new(revision as u64);
    let knowledge_snapshot = KnowledgeSnapshotRef {
        story_id: story_id.clone(),
        pack_digest: digest,
        base_revision,
        knowledge_id_high_water: crate::domain::knowledge::KnowledgeIdHighWater::new(
            knowledge_id_high_water.max(0) as u64
        ),
    };
    tx.commit().await.map_err(SqliteStoreError::from)?;
    StoryReadSnapshot::try_from_parts(StoryReadSnapshotParts {
        story_id: story_id.clone(),
        base_revision,
        pack: pack_ref,
        story_title,
        story_profile,
        instance_settings,
        roles,
        relationships,
        narrative_definition,
        narrative_state,
        fact_values,
        story_continuity,
        active_constraints,
        entity_catalog,
        topic_dictionary,
        knowledge_snapshot,
        role_id_high_water: crate::domain::ids::RoleIdHighWater::new(role_id_high_water.max(0) as u64),
    })
    .map_err(|_| StoreError::ConstraintViolation {
        constraint: "story_snapshot".into(),
    })
}

fn projection_limit(items: usize, item_bytes: usize, overhead: usize) -> Result<usize, StoreError> {
    items
        .checked_mul(item_bytes)
        .and_then(|value| value.checked_add(overhead))
        .ok_or(StoreError::LimitExceeded {
            limit: "snapshot_projection",
        })
}

fn ensure_projection_length(actual: i64, maximum: usize, limit: &'static str) -> Result<(), StoreError> {
    if actual < 0 {
        return Err(StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
        });
    }
    if actual as u64 > maximum as u64 {
        return Err(StoreError::LimitExceeded { limit });
    }
    Ok(())
}

fn parse_entity(kind: &str, key: &str) -> Result<KnowledgeEntity, StoreError> {
    Ok(match kind {
        "world" => KnowledgeEntity::World(key.into()),
        "role" => KnowledgeEntity::Role(RoleId::try_new(key.to_owned()).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
        })?),
        "location" => KnowledgeEntity::Location(key.into()),
        "scene" => KnowledgeEntity::Scene(key.into()),
        "narrative_node" => KnowledgeEntity::NarrativeNode(key.into()),
        "event" => KnowledgeEntity::Event(key.into()),
        _ => {
            return Err(StoreError::Serialization {
                kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
            });
        }
    })
}
