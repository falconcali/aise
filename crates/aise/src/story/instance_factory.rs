use crate::domain::asset::frozen_ref::FrozenCharacterAssetRef;
use crate::domain::asset::ids::{PackId, PlayerId, StoryRoleKey};
use crate::domain::ids::{CharacterId, ConstraintId, FactId, MemoryId, StoryId, StoryRevision};
use crate::domain::knowledge::KnowledgeEntry;
use crate::domain::knowledge::fact::{Proposition, WorldFact};
use crate::domain::knowledge::memory::MemoryEntry;
use crate::domain::knowledge::query::KnowledgeSource;
use crate::domain::knowledge::rumor::{Claim, SharedRumor, TruthValue};
use crate::domain::narrative_graph::state::NarrativeRuntimeState;
use crate::domain::story_instance::binding::{RoleBinding, RoleController};
use crate::domain::story_instance::constraint::{ActiveStoryConstraint, StoryConstraintSource};
use crate::domain::story_instance::info::StoryInfo;
use crate::domain::story_instance::snapshot::NarrativeConditionStateView;
use crate::domain::story_instance::state::{CharacterInstanceState, CurrentScene, InstanceSettings, RelationshipState};
use crate::persistence::asset_store::{AssetStore, FrozenStoryPack};
use crate::persistence::store::{MaterializedStoryInstanceSpec, Store, StoreError};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct CreateStoryInstanceSpec {
    pub pack_id: PackId,
    pub player_id: PlayerId,
    pub player_role_key: StoryRoleKey,
    pub player_character: Option<FrozenCharacterAssetRef>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone)]
pub struct StoryInstantiationLimits {
    pub max_roles: usize,
    pub max_facts: usize,
    pub max_rumors: usize,
    pub max_memories: usize,
    pub max_relationships: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum StoryInstantiationError {
    #[error("story pack was not found")]
    PackNotFound,
    #[error("story role was not found")]
    RoleNotFound,
    #[error("story role is not playable")]
    RoleNotPlayable,
    #[error("character asset was not found")]
    CharacterNotFound,
    #[error("story materialization reference is invalid: {code}")]
    InvalidReference { code: &'static str },
    #[error("story instantiation limit exceeded: {limit}")]
    LimitExceeded { limit: &'static str },
    #[error("story store operation failed")]
    Store(StoreError),
}

pub struct StoryInstanceFactory {
    asset_store: Arc<dyn AssetStore>,
    store: Arc<dyn Store>,
    limits: StoryInstantiationLimits,
}

impl StoryInstanceFactory {
    pub fn new(asset_store: Arc<dyn AssetStore>, store: Arc<dyn Store>, limits: StoryInstantiationLimits) -> Self {
        Self {
            asset_store,
            store,
            limits,
        }
    }

    pub async fn create(&self, spec: CreateStoryInstanceSpec) -> Result<StoryInfo, StoryInstantiationError> {
        let frozen = self.asset_store.load_pack(&spec.pack_id).await.map_err(|error| match error {
            StoreError::NotFound => StoryInstantiationError::PackNotFound,
            other => StoryInstantiationError::Store(other),
        })?;
        let materialized = self.materialize(&frozen, &spec)?;
        self.store
            .create_story_instance(&materialized)
            .await
            .map_err(StoryInstantiationError::Store)
    }

    fn materialize(
        &self,
        frozen: &FrozenStoryPack,
        spec: &CreateStoryInstanceSpec,
    ) -> Result<MaterializedStoryInstanceSpec, StoryInstantiationError> {
        let pack = &frozen.pack;
        if !pack.play.playable_role_keys.contains(&spec.player_role_key) {
            return Err(StoryInstantiationError::RoleNotPlayable);
        }
        if !pack.roles.contains_key(&spec.player_role_key) {
            return Err(StoryInstantiationError::RoleNotFound);
        }
        enforce_limit(pack.roles.len(), self.limits.max_roles, "max_roles")?;
        enforce_limit(frozen.resolved_world_book.facts.len(), self.limits.max_facts, "max_facts")?;
        enforce_limit(frozen.resolved_world_book.rumors.len(), self.limits.max_rumors, "max_rumors")?;
        let memory_count = pack.roles.values().try_fold(0usize, |count, role| {
            count
                .checked_add(role.seed_memories.len())
                .ok_or(StoryInstantiationError::LimitExceeded { limit: "max_memories" })
        })?;
        enforce_limit(memory_count, self.limits.max_memories, "max_memories")?;
        let relationship_count = pack.roles.values().try_fold(0usize, |count, role| {
            count
                .checked_add(role.initial_relationships.len())
                .ok_or(StoryInstantiationError::LimitExceeded {
                    limit: "max_relationships",
                })
        })?;
        enforce_limit(relationship_count, self.limits.max_relationships, "max_relationships")?;
        let story_id = StoryId::try_new(format!("story-{}", uuid::Uuid::new_v4())).map_err(|_| {
            StoryInstantiationError::Store(StoreError::ConstraintViolation {
                constraint: "story_id".into(),
            })
        })?;
        let selected_player_asset = spec
            .player_character
            .as_ref()
            .map(|asset| resolve_custom_character(frozen, asset))
            .transpose()?;
        let mut bindings = BTreeMap::new();
        let mut characters = BTreeMap::new();
        for (role_key, role) in &pack.roles {
            let asset_key = if role_key == &spec.player_role_key {
                selected_player_asset
                    .as_ref()
                    .map(|asset| asset.character_key.clone())
                    .or_else(|| pack.default_cast.get(role_key).map(|cast| cast.character_ref.clone()))
            } else {
                pack.default_cast.get(role_key).map(|cast| cast.character_ref.clone())
            }
            .ok_or(StoryInstantiationError::CharacterNotFound)?;
            let card = frozen
                .resolved_characters
                .get(&asset_key)
                .ok_or(StoryInstantiationError::CharacterNotFound)?;
            let character_asset = freeze_character(card)?;
            if let Some(selected) = selected_player_asset.as_ref() {
                if role_key == &spec.player_role_key && &character_asset != selected {
                    return Err(StoryInstantiationError::CharacterNotFound);
                }
            }
            let character_id = CharacterId::from(format!("{}:character:{}", story_id.as_str(), role_key.as_str()));
            let controller = if role_key == &spec.player_role_key {
                RoleController::Player(spec.player_id.clone())
            } else {
                RoleController::Ai
            };
            let binding = RoleBinding {
                role_key: role_key.clone(),
                character_id: character_id.clone(),
                character_asset,
                controller,
                bound_at_ms: spec.created_at_ms,
            };
            if bindings.insert(role_key.clone(), binding).is_some()
                || characters
                    .insert(
                        character_id.clone(),
                        CharacterInstanceState {
                            character_id,
                            role_key: role_key.clone(),
                            location: role.initial_state.location.clone(),
                            goals: role.initial_state.goals.clone(),
                            attributes: role.initial_state.attributes.clone(),
                        },
                    )
                    .is_some()
            {
                return Err(StoryInstantiationError::InvalidReference {
                    code: "duplicate_materialized_character",
                });
            }
        }
        let relationships = materialize_relationships(pack, &bindings)?;
        let knowledge = materialize_knowledge(frozen, &story_id, &bindings, spec.created_at_ms)?;
        let active_constraints = pack
            .constraints
            .iter()
            .map(|(key, definition)| {
                Ok(ActiveStoryConstraint {
                    id: ConstraintId::try_new(format!("{}:seed:constraint:{}", story_id.as_str(), key.as_str()))
                        .map_err(|_| StoryInstantiationError::InvalidReference {
                            code: "constraint_id_invalid",
                        })?,
                    source: StoryConstraintSource {
                        pack_id: frozen.pack_id.clone(),
                        constraint_key: key.clone(),
                    },
                    scope: definition.scope.clone(),
                    requirement: definition.requirement.clone(),
                    lifecycle: definition.lifecycle.clone(),
                })
            })
            .collect::<Result<Vec<_>, StoryInstantiationError>>()?;
        let mut present_character_ids = pack
            .start
            .role_openings
            .keys()
            .map(|role| {
                bindings.get(role).map(|binding| binding.character_id.clone()).ok_or(
                    StoryInstantiationError::InvalidReference {
                        code: "start_role_binding_missing",
                    },
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        present_character_ids.sort();
        present_character_ids.dedup();
        let opening = pack.start.role_openings.get(&spec.player_role_key).cloned().ok_or(
            StoryInstantiationError::InvalidReference {
                code: "player_opening_missing",
            },
        )?;
        Ok(MaterializedStoryInstanceSpec {
            story_id,
            pack: frozen.frozen_ref(),
            settings: InstanceSettings::default(),
            bindings,
            characters,
            relationships,
            knowledge,
            scene: CurrentScene {
                scene_key: pack.start.scene_key.clone(),
                location_key: pack.start.location_key.clone(),
                time: pack.start.time.clone(),
                description: pack.start.description.clone(),
                present_character_ids,
            },
            current_perceptions: Vec::new(),
            narrative_state: NarrativeRuntimeState::initial(),
            condition_state: NarrativeConditionStateView {
                occurred_event_keys: BTreeSet::new(),
                player_action_event_keys: BTreeSet::new(),
                fact_values: BTreeMap::new(),
            },
            active_constraints,
            opening,
            created_at_ms: spec.created_at_ms,
        })
    }
}

fn materialize_relationships(
    pack: &crate::domain::asset::story_pack::StoryPack,
    bindings: &BTreeMap<StoryRoleKey, RoleBinding>,
) -> Result<Vec<RelationshipState>, StoryInstantiationError> {
    let mut relationships = Vec::new();
    let mut keys = BTreeSet::new();
    for (source_role, role) in &pack.roles {
        let source = bindings.get(source_role).ok_or(StoryInstantiationError::InvalidReference {
            code: "relationship_source_missing",
        })?;
        for seed in &role.initial_relationships {
            let target = bindings
                .get(&seed.target_role_key)
                .ok_or(StoryInstantiationError::InvalidReference {
                    code: "relationship_target_missing",
                })?;
            let relationship = RelationshipState {
                source_character_id: source.character_id.clone(),
                target_character_id: target.character_id.clone(),
                kind: seed.kind.clone(),
                trust: seed.trust,
            };
            if !keys.insert(relationship.key()) {
                return Err(StoryInstantiationError::InvalidReference {
                    code: "duplicate_relationship",
                });
            }
            relationships.push(relationship);
        }
    }
    relationships.sort_by_key(RelationshipState::key);
    Ok(relationships)
}

fn materialize_knowledge(
    frozen: &FrozenStoryPack,
    story_id: &StoryId,
    bindings: &BTreeMap<StoryRoleKey, RoleBinding>,
    created_at_ms: i64,
) -> Result<Vec<KnowledgeEntry>, StoryInstantiationError> {
    let source = KnowledgeSource::Seed {
        pack_id: frozen.pack_id.clone(),
        pack_digest: frozen.digest.clone(),
    };
    let mut entries = Vec::new();
    let mut ids = BTreeSet::new();
    for (key, seed) in &frozen.resolved_world_book.facts {
        let id = FactId::from(format!("{}:seed:fact:{}", story_id.as_str(), key.as_str()));
        let proposition = seed.proposition.as_ref().map(|value| Proposition {
            subject: value.subject.clone(),
            predicate: value.predicate.clone(),
            value: value.value.clone(),
        });
        let entry = KnowledgeEntry::Fact(WorldFact {
            id,
            key: Some(key.clone()),
            text: seed.content.clone(),
            proposition,
            entities: canonical(seed.entities.clone()),
            topics: canonical(seed.topics.clone()),
            salience: seed.salience,
            source: source.clone(),
            story_revision: StoryRevision::new(0),
        });
        insert_entry(&mut entries, &mut ids, entry)?;
    }
    for (key, seed) in &frozen.resolved_world_book.rumors {
        let id = crate::domain::ids::RumorId::from(format!("{}:seed:rumor:{}", story_id.as_str(), key.as_str()));
        let claim = seed.claim.as_ref().map(|value| Claim {
            subject: value.subject.clone(),
            predicate: value.predicate.clone(),
            value: value.value.clone(),
        });
        let entry = KnowledgeEntry::Rumor(SharedRumor {
            id,
            key: Some(key.clone()),
            content: seed.content.clone(),
            claim,
            entities: canonical(seed.entities.clone()),
            topics: canonical(seed.topics.clone()),
            salience: seed.salience,
            source_role_key: None,
            source_character_id: None,
            truth_value: TruthValue::Unverified,
            source: source.clone(),
            story_revision: StoryRevision::new(0),
        });
        insert_entry(&mut entries, &mut ids, entry)?;
    }
    for (role_key, role) in &frozen.pack.roles {
        let binding = bindings.get(role_key).ok_or(StoryInstantiationError::InvalidReference {
            code: "memory_owner_binding_missing",
        })?;
        for seed in &role.seed_memories {
            let id = MemoryId::from(format!(
                "{}:seed:memory:{}:{}",
                story_id.as_str(),
                role_key.as_str(),
                seed.memory_key.as_str()
            ));
            let entities = canonical(vec![
                crate::domain::asset::entity::KnowledgeEntity::Role(role_key.clone()),
                crate::domain::asset::entity::KnowledgeEntity::Character(binding.character_id.clone()),
            ]);
            let entry = KnowledgeEntry::Memory(MemoryEntry {
                id,
                owner: binding.character_id.clone(),
                kind: seed.kind.clone(),
                content: seed.content.clone(),
                entities,
                topics: canonical(seed.topics.clone()),
                salience: seed.salience,
                source: source.clone(),
                story_revision: StoryRevision::new(0),
                created_at_ms,
            });
            insert_entry(&mut entries, &mut ids, entry)?;
        }
    }
    Ok(entries)
}

fn insert_entry(
    entries: &mut Vec<KnowledgeEntry>,
    ids: &mut BTreeSet<crate::domain::knowledge::KnowledgeSourceId>,
    entry: KnowledgeEntry,
) -> Result<(), StoryInstantiationError> {
    if !ids.insert(entry.source_id()) {
        return Err(StoryInstantiationError::InvalidReference {
            code: "duplicate_knowledge_id",
        });
    }
    entries.push(entry);
    Ok(())
}

fn resolve_custom_character(
    frozen: &FrozenStoryPack,
    requested: &FrozenCharacterAssetRef,
) -> Result<FrozenCharacterAssetRef, StoryInstantiationError> {
    let card = frozen
        .resolved_characters
        .get(&requested.character_key)
        .ok_or(StoryInstantiationError::CharacterNotFound)?;
    let resolved = freeze_character(card)?;
    if &resolved != requested {
        return Err(StoryInstantiationError::CharacterNotFound);
    }
    Ok(resolved)
}

fn freeze_character(
    card: &crate::domain::asset::character_card::CharacterCard,
) -> Result<FrozenCharacterAssetRef, StoryInstantiationError> {
    let bytes = serde_json::to_vec(card).map_err(|_| StoryInstantiationError::InvalidReference {
        code: "character_asset_serialization",
    })?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let mut digest = [0u8; 32];
    digest.copy_from_slice(&hasher.finalize());
    Ok(FrozenCharacterAssetRef {
        character_key: card.character_key.clone(),
        version: card.meta.version.clone(),
        digest: crate::domain::asset::ids::Sha256Digest::from_bytes(digest),
    })
}

fn canonical<T: Ord>(mut values: Vec<T>) -> Vec<T> {
    values.sort();
    values.dedup();
    values
}

fn enforce_limit(actual: usize, maximum: usize, limit: &'static str) -> Result<(), StoryInstantiationError> {
    if actual > maximum {
        return Err(StoryInstantiationError::LimitExceeded { limit });
    }
    Ok(())
}
