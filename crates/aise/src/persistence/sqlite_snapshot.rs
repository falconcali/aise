use crate::core::turn_data::SnapshotLimits;
use crate::domain::asset::character_card::CharacterCard;
use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::frozen_ref::FrozenStoryPackRef;
use crate::domain::asset::ids::{PackId, Sha256Digest, StoryRoleKey, TopicKey};
use crate::domain::asset::story_pack::{StoryPack, StoryProfile, StoryRole};
use crate::domain::asset::validation::BoundedText;
use crate::domain::asset::world_book::{TopicDefinition, WorldBook};
use crate::domain::ids::{CharacterId, StoryId, StoryRevision, TurnId};
use crate::domain::narrative::{StoryContinuity, StorySegment, StorySummary};
use crate::domain::narrative_graph::definition::NarrativeGraphDefinition;
use crate::domain::narrative_graph::state::NarrativeRuntimeState;
use crate::domain::story_instance::binding::RoleBinding;
use crate::domain::story_instance::constraint::ActiveStoryConstraint;
use crate::domain::story_instance::snapshot::{KnowledgeSnapshotRef, NarrativeConditionStateView, StoryReadSnapshot};
use crate::domain::story_instance::state::{CharacterInstanceState, CurrentScene, InstanceSettings, RelationshipState};
use crate::persistence::sqlite_error::SqliteStoreError;
use crate::persistence::store::StoreError;
use sqlx::SqlitePool;
use std::collections::{BTreeMap, BTreeSet};

type StoryInstanceRow = (i64, String, String, String, String, String, String, String, String);
type StoryPackRow = (Vec<u8>, String, Vec<u8>, Vec<u8>);

pub(crate) async fn load_story_snapshot(
    pool: &SqlitePool,
    story_id: &StoryId,
    limits: SnapshotLimits,
) -> Result<StoryReadSnapshot, StoreError> {
    let mut tx = pool.begin().await.map_err(SqliteStoreError::from)?;
    let row: Option<StoryInstanceRow> = sqlx::query_as(
        "SELECT s.revision, i.pack_id, i.bindings_json, i.characters_json, i.relationships_json, \
                i.narrative_state_json, s.current_scene, s.story_summary, s.active_constraints \
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
        bindings_json,
        characters_json,
        relationships_json,
        narrative_state_json,
        current_scene_json,
        story_summary_json,
        active_constraints_json,
    )) = row
    else {
        tx.rollback().await.map_err(SqliteStoreError::from)?;
        return Err(StoreError::NotFound);
    };
    let pack_row: Option<StoryPackRow> =
        sqlx::query_as("SELECT pack_json, digest, characters_json, world_book_json FROM story_packs WHERE pack_id = ?")
            .bind(&pack_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(SqliteStoreError::from)?;
    let Some((pack_json, digest_raw, pack_characters_json, world_book_json)) = pack_row else {
        tx.rollback().await.map_err(SqliteStoreError::from)?;
        return Err(StoreError::NotFound);
    };
    let pack: StoryPack = serde_json::from_slice(&pack_json).map_err(|_| StoreError::Serialization {
        kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
    })?;
    let digest = Sha256Digest::try_new(&digest_raw).map_err(|_| StoreError::Serialization {
        kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
    })?;
    let resolved_characters: BTreeMap<crate::domain::asset::ids::CharacterAssetKey, CharacterCard> =
        serde_json::from_slice(&pack_characters_json).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidCharacterState,
        })?;
    let world_book: WorldBook = serde_json::from_slice(&world_book_json).map_err(|_| StoreError::Serialization {
        kind: crate::persistence::store::StoreSerializationErrorKind::InvalidWorldState,
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
    if relationships.len() > limits.max_relationships {
        return Err(StoreError::ConstraintViolation {
            constraint: "max_relationships".into(),
        });
    }
    let narrative_state: NarrativeRuntimeState =
        serde_json::from_str(&narrative_state_json).map_err(|_| StoreError::Serialization {
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
    let summary: StorySummary = serde_json::from_str(&story_summary_json).unwrap_or_default();
    let active_constraints: Vec<ActiveStoryConstraint> =
        serde_json::from_str(&active_constraints_json).unwrap_or_default();
    if active_constraints.len() > limits.max_constraints {
        return Err(StoreError::ConstraintViolation {
            constraint: "max_constraints".into(),
        });
    }
    let segment_rows: Vec<(String, i64, String)> = sqlx::query_as(
        "SELECT id, sequence, story_text FROM story_turns \
         WHERE world_id = ? AND sequence IS NOT NULL \
         ORDER BY sequence DESC LIMIT ?",
    )
    .bind(story_id.as_str())
    .bind(limits.continuity.max_recent_segments as i64)
    .fetch_all(&mut *tx)
    .await
    .map_err(SqliteStoreError::from)?;
    let summary_through = summary.summarized_through.map(|sequence| sequence.get()).unwrap_or(0);
    let mut recent_segments = Vec::new();
    for (id, sequence, story_text) in segment_rows.into_iter().rev() {
        if (sequence as u64) <= summary_through {
            continue;
        }
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
    let entity_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT DISTINCT entity_kind, entity_key FROM knowledge_entry_entities \
         WHERE story_id = ? ORDER BY entity_kind ASC, entity_key ASC LIMIT ?",
    )
    .bind(story_id.as_str())
    .bind(limits.max_entity_catalog as i64)
    .fetch_all(&mut *tx)
    .await
    .map_err(SqliteStoreError::from)?;
    let mut entity_catalog = Vec::new();
    for (kind, key) in entity_rows {
        if let Some(entity) = parse_entity(&kind, &key) {
            entity_catalog.push(entity);
        }
    }
    let topic_dictionary: BTreeMap<TopicKey, TopicDefinition> = world_book.topics.clone();
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
    for (role_key, binding) in &bindings {
        let cast = pack.default_cast.get(role_key);
        if let Some(cast) = cast {
            if let Some(card) = resolved_characters.get(&cast.character_ref) {
                character_cards.insert(binding.character_id.clone(), card.clone());
            }
        }
    }
    let role_definitions: BTreeMap<StoryRoleKey, StoryRole> = pack.roles.clone();
    if role_definitions.len() > limits.max_roles {
        return Err(StoreError::ConstraintViolation {
            constraint: "max_roles".into(),
        });
    }
    let story_profile: StoryProfile = pack.story.clone();
    if story_profile.premise.as_str().len() > limits.max_story_profile_bytes {
        return Err(StoreError::ConstraintViolation {
            constraint: "max_story_profile_bytes".into(),
        });
    }
    let pack_ref = FrozenStoryPackRef {
        pack_id: PackId::from(pack_id),
        pack_key: pack.meta.pack_key.clone(),
        version: pack.meta.version.clone(),
        digest: digest.clone(),
    };
    let base_revision = StoryRevision::new(revision as u64);
    let knowledge_snapshot = KnowledgeSnapshotRef {
        story_id: story_id.clone(),
        pack_digest: digest,
        base_revision,
    };
    let narrative_definition: NarrativeGraphDefinition = pack.narrative.clone();
    tx.commit().await.map_err(SqliteStoreError::from)?;
    StoryReadSnapshot::try_new(
        story_id.clone(),
        base_revision,
        pack_ref,
        story_profile,
        InstanceSettings::default(),
        role_definitions,
        bindings,
        character_cards,
        character_states,
        current_scene,
        relationships,
        Vec::new(),
        narrative_definition,
        narrative_state,
        NarrativeConditionStateView {
            occurred_event_keys: BTreeSet::new(),
            player_action_event_keys: BTreeSet::new(),
            fact_values: BTreeMap::new(),
        },
        story_continuity,
        active_constraints,
        entity_catalog,
        topic_dictionary,
        knowledge_snapshot,
    )
    .map_err(|_| StoreError::ConstraintViolation {
        constraint: "story_snapshot".into(),
    })
}

fn parse_entity(kind: &str, key: &str) -> Option<KnowledgeEntity> {
    Some(match kind {
        "world" => KnowledgeEntity::World(key.into()),
        "role" => KnowledgeEntity::Role(key.into()),
        "character" => KnowledgeEntity::Character(CharacterId::from(key.to_owned())),
        "location" => KnowledgeEntity::Location(key.into()),
        "scene" => KnowledgeEntity::Scene(key.into()),
        "narrative_node" => KnowledgeEntity::NarrativeNode(key.into()),
        "event" => KnowledgeEntity::Event(key.into()),
        _ => return None,
    })
}
