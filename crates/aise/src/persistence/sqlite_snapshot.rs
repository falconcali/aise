use crate::domain::asset::character_card::CharacterCard;
use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::frozen_ref::FrozenStoryPackRef;
use crate::domain::asset::ids::{PackId, Sha256Digest, StoryRoleKey, TopicKey};
use crate::domain::asset::story_pack::{StoryProfile, StoryRole};
use crate::domain::asset::validation::BoundedText;
use crate::domain::asset::world_book::TopicDefinition;
use crate::domain::ids::{CharacterId, StoryId, StoryRevision, TurnId};
use crate::domain::narrative::{StoryContinuity, StorySegment, StorySummary};
use crate::domain::narrative_graph::definition::NarrativeGraphDefinition;
use crate::domain::narrative_graph::state::NarrativeRuntimeState;
use crate::domain::story_instance::binding::RoleBinding;
use crate::domain::story_instance::constraint::ActiveStoryConstraint;
use crate::domain::story_instance::snapshot::{
    KnowledgeSnapshotRef, NarrativeConditionStateView, StoryReadSnapshot, StoryReadSnapshotParts,
};
use crate::domain::story_instance::state::{CharacterInstanceState, CurrentScene, InstanceSettings, RelationshipState};
use crate::domain::turn::SnapshotLimits;
use crate::persistence::sqlite_error::SqliteStoreError;
use crate::persistence::store::StoreError;
use sqlx::SqlitePool;
use std::collections::BTreeMap;

type StoryInstanceRow = (
    i64,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
);
type StoryPackRow = (String, String, String, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>);
type InstanceProjectionLengths = (String, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64);
type PackProjectionLengths = (i64, i64, i64, i64, i64);

pub(crate) async fn load_story_snapshot(
    pool: &SqlitePool,
    story_id: &StoryId,
    limits: SnapshotLimits,
) -> Result<StoryReadSnapshot, StoreError> {
    let mut tx = pool.begin().await.map_err(SqliteStoreError::from)?;
    let instance_lengths: Option<InstanceProjectionLengths> = sqlx::query_as(
        "SELECT i.pack_id, length(i.settings_json), length(i.bindings_json), length(i.characters_json), \
                length(i.relationships_json), length(i.current_perceptions_json), length(i.narrative_state_json), \
                length(i.condition_state_json), length(s.current_scene), length(s.story_summary), \
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
        bindings_len,
        characters_len,
        relationships_len,
        perceptions_len,
        narrative_state_len,
        condition_state_len,
        scene_len,
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
        bindings_len,
        projection_limit(limits.max_roles, limits.max_character_bytes, 1024)?,
        "bindings_json",
    )?;
    ensure_projection_length(
        characters_len,
        projection_limit(limits.max_characters, limits.max_character_bytes, 1024)?,
        "characters_json",
    )?;
    ensure_projection_length(
        relationships_len,
        projection_limit(limits.max_relationships, limits.max_character_bytes, 1024)?,
        "relationships_json",
    )?;
    ensure_projection_length(
        perceptions_len,
        projection_limit(limits.max_current_perceptions, limits.max_perception_bytes, 1024)?,
        "current_perceptions_json",
    )?;
    ensure_projection_length(
        narrative_state_len,
        projection_limit(limits.max_narrative_nodes, limits.max_character_bytes, 1024)?,
        "narrative_state_json",
    )?;
    ensure_projection_length(
        condition_state_len,
        projection_limit(
            limits
                .max_condition_event_keys
                .checked_add(limits.max_condition_fact_values)
                .ok_or(StoreError::LimitExceeded {
                    limit: "condition_state_json",
                })?,
            limits.max_constraint_bytes,
            1024,
        )?,
        "condition_state_json",
    )?;
    ensure_projection_length(
        scene_len,
        projection_limit(limits.max_scene_characters, 256, limits.max_scene_bytes)?,
        "current_scene",
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
        "SELECT length(story_profile_json), length(role_definitions_json), length(narrative_definition_json), \
                length(topic_dictionary_json), length(resolved_characters_json) \
         FROM story_packs WHERE pack_id = ?",
    )
    .bind(&projection_pack_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(SqliteStoreError::from)?;
    let Some((profile_len, roles_len, narrative_len, topics_len, cards_len)) = pack_lengths else {
        return Err(StoreError::NotFound);
    };
    ensure_projection_length(profile_len, limits.max_story_profile_bytes, "story_profile_json")?;
    ensure_projection_length(
        roles_len,
        projection_limit(limits.max_roles, limits.max_character_bytes, 1024)?,
        "role_definitions_json",
    )?;
    ensure_projection_length(
        narrative_len,
        projection_limit(limits.max_narrative_nodes, limits.max_character_bytes, 1024)?,
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
        projection_limit(topic_items, limits.max_character_bytes, 1024)?,
        "topic_dictionary_json",
    )?;
    ensure_projection_length(
        cards_len,
        projection_limit(limits.max_characters, limits.max_character_bytes, 1024)?,
        "resolved_characters_json",
    )?;
    let row: Option<StoryInstanceRow> = sqlx::query_as(
        "SELECT s.revision, i.pack_id, i.settings_json, i.bindings_json, i.characters_json, \
                i.relationships_json, i.current_perceptions_json, i.narrative_state_json, \
                i.condition_state_json, s.current_scene, s.story_summary, s.active_constraints \
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
        bindings_json,
        characters_json,
        relationships_json,
        current_perceptions_json,
        narrative_state_json,
        condition_state_json,
        current_scene_json,
        story_summary_json,
        active_constraints_json,
    )) = row
    else {
        tx.rollback().await.map_err(SqliteStoreError::from)?;
        return Err(StoreError::NotFound);
    };
    let pack_row: Option<StoryPackRow> = sqlx::query_as(
        "SELECT pack_key, version, digest, story_profile_json, role_definitions_json, narrative_definition_json, \
                topic_dictionary_json, resolved_characters_json \
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
        role_definitions_json,
        narrative_definition_json,
        topic_dictionary_json,
        pack_characters_json,
    )) = pack_row
    else {
        tx.rollback().await.map_err(SqliteStoreError::from)?;
        return Err(StoreError::NotFound);
    };
    let story_profile: StoryProfile =
        serde_json::from_slice(&story_profile_json).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
        })?;
    let digest = Sha256Digest::try_new(&digest_raw).map_err(|_| StoreError::Serialization {
        kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
    })?;
    let resolved_characters: BTreeMap<crate::domain::asset::ids::CharacterAssetKey, CharacterCard> =
        serde_json::from_slice(&pack_characters_json).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidCharacterState,
        })?;
    let role_definitions: BTreeMap<StoryRoleKey, StoryRole> =
        serde_json::from_slice(&role_definitions_json).map_err(|_| StoreError::Serialization {
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
    let bindings: BTreeMap<StoryRoleKey, RoleBinding> =
        serde_json::from_str(&bindings_json).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
        })?;
    let character_states: BTreeMap<CharacterId, CharacterInstanceState> = serde_json::from_str(&characters_json)
        .map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidCharacterState,
        })?;
    if character_states.len() > limits.max_characters {
        return Err(StoreError::ConstraintViolation {
            constraint: "max_characters".into(),
        });
    }
    let relationships: Vec<RelationshipState> =
        serde_json::from_str(&relationships_json).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
        })?;
    let current_perceptions =
        serde_json::from_str(&current_perceptions_json).map_err(|_| StoreError::Serialization {
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
    let condition_state: NarrativeConditionStateView =
        serde_json::from_str(&condition_state_json).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
        })?;
    let current_scene: CurrentScene =
        serde_json::from_str(&current_scene_json).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
        })?;
    if current_scene.description.as_str().len() > limits.max_scene_bytes {
        return Err(StoreError::ConstraintViolation {
            constraint: "max_scene_bytes".into(),
        });
    }
    if current_scene.present_character_ids.len() > limits.max_scene_characters {
        return Err(StoreError::ConstraintViolation {
            constraint: "max_scene_characters".into(),
        });
    }
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
    let segment_rows: Vec<(String, i64, String)> = sqlx::query_as(
        "SELECT id, sequence, story_text FROM story_turns \
         WHERE world_id = ? AND sequence > ? \
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
    for (id, sequence, story_text) in segment_rows {
        let turn_id = TurnId::try_new(id).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidTurnResult,
        })?;
        let sequence =
            crate::domain::StorySequence::try_new(sequence as u64).map_err(|_| StoreError::Serialization {
                kind: crate::persistence::store::StoreSerializationErrorKind::InvalidTurnResult,
            })?;
        let text = BoundedText::try_new(story_text, "recent_segment", limits.continuity.max_recent_segment_bytes)
            .map_err(|_| StoreError::ConstraintViolation {
                constraint: "max_recent_segment_bytes".into(),
            })?;
        recent_segments.push(StorySegment {
            sequence,
            turn_id,
            text,
        });
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
    let mut character_cards = BTreeMap::new();
    for binding in bindings.values() {
        if let Some(card) = resolved_characters.get(&binding.character_asset.character_key) {
            character_cards.insert(binding.character_id.clone(), card.clone());
        }
    }
    if role_definitions.len() > limits.max_roles {
        return Err(StoreError::ConstraintViolation {
            constraint: "max_roles".into(),
        });
    }
    if story_profile.premise.as_str().len() > limits.max_story_profile_bytes {
        return Err(StoreError::ConstraintViolation {
            constraint: "max_story_profile_bytes".into(),
        });
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
    };
    tx.commit().await.map_err(SqliteStoreError::from)?;
    StoryReadSnapshot::try_from_parts(StoryReadSnapshotParts {
        story_id: story_id.clone(),
        base_revision,
        pack: pack_ref,
        story_profile,
        instance_settings,
        role_definitions,
        role_bindings: bindings,
        character_cards,
        character_states,
        current_scene,
        relationships,
        current_perceptions,
        narrative_definition,
        narrative_state,
        condition_state,
        story_continuity,
        active_constraints,
        entity_catalog,
        topic_dictionary,
        knowledge_snapshot,
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
        "role" => KnowledgeEntity::Role(key.into()),
        "character" => KnowledgeEntity::Character(CharacterId::from(key.to_owned())),
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
