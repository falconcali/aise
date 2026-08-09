use crate::domain::asset::frozen_ref::FrozenCharacterAssetRef;
use crate::domain::asset::ids::{PackId, PlayerId, Sha256Digest, StoryRoleKey};
use crate::domain::ids::{CharacterId, StoryId, StoryRevision};
use crate::domain::knowledge::fact::WorldFact;
use crate::domain::knowledge::memory::MemoryEntry;
use crate::domain::knowledge::query::KnowledgeSource;
use crate::domain::knowledge::rumor::SharedRumor;
use crate::domain::narrative_graph::state::NarrativeRuntimeState;
use crate::domain::story_instance::binding::{RoleBinding, StoryInstanceBinding};
use crate::domain::story_instance::info::StoryInfo;
use crate::domain::story_instance::state::CurrentScene;
use crate::domain::story_instance::state::{CharacterInstanceState, RelationshipState};
use crate::persistence::asset_store::AssetStore;
use crate::persistence::store::{MaterializedStoryInstanceSpec, Store, StoreError};
use std::collections::BTreeMap;
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
        let pack = &frozen.pack;
        if !pack.play.playable_role_keys.contains(&spec.player_role_key) {
            return Err(StoryInstantiationError::RoleNotPlayable);
        }
        if !pack.roles.contains_key(&spec.player_role_key) {
            return Err(StoryInstantiationError::RoleNotFound);
        }
        let roles_count = pack.roles.len();
        if roles_count > self.limits.max_roles {
            return Err(StoryInstantiationError::LimitExceeded { limit: "max_roles" });
        }
        let mut bindings: BTreeMap<StoryRoleKey, RoleBinding> = BTreeMap::new();
        let mut characters: BTreeMap<CharacterId, CharacterInstanceState> = BTreeMap::new();
        let story_id = StoryId::try_new(format!("story-{}", uuid::Uuid::new_v4())).map_err(|_| {
            StoryInstantiationError::Store(StoreError::ConstraintViolation {
                constraint: "story_id".into(),
            })
        })?;
        let player_binding = RoleBinding {
            role_key: spec.player_role_key.clone(),
            player_id: Some(spec.player_id.clone()),
            character_id: CharacterId::from(format!("player-{}", spec.player_id.as_str())),
            bound_at_ms: spec.created_at_ms,
        };
        bindings.insert(spec.player_role_key.clone(), player_binding);
        let player_role = &pack.roles[&spec.player_role_key];
        characters.insert(
            bindings[&spec.player_role_key].character_id.clone(),
            CharacterInstanceState {
                character_id: bindings[&spec.player_role_key].character_id.clone(),
                role_key: spec.player_role_key.clone(),
                location: player_role.initial_state.location.clone(),
                goals: player_role.initial_state.goals.clone(),
                attributes: player_role.initial_state.attributes.clone(),
            },
        );
        for (role_key, role) in &pack.roles {
            if role_key == &spec.player_role_key {
                continue;
            }
            let character_id = CharacterId::from(format!("ai-{}", role_key.as_str()));
            let binding = RoleBinding {
                role_key: role_key.clone(),
                player_id: None,
                character_id: character_id.clone(),
                bound_at_ms: spec.created_at_ms,
            };
            bindings.insert(role_key.clone(), binding);
            characters.insert(
                character_id.clone(),
                CharacterInstanceState {
                    character_id,
                    role_key: role_key.clone(),
                    location: role.initial_state.location.clone(),
                    goals: role.initial_state.goals.clone(),
                    attributes: role.initial_state.attributes.clone(),
                },
            );
        }
        let binding = StoryInstanceBinding {
            story_id: story_id.clone(),
            pack_id: spec.pack_id.clone(),
            revision: StoryRevision::new(0),
            role_bindings: bindings.values().cloned().collect(),
        };
        let _ = binding;
        let opening = pack
            .start
            .role_openings
            .get(&spec.player_role_key)
            .map(|opening| opening.to_string())
            .unwrap_or_default();
        let present_character_ids = characters.keys().cloned().collect();
        let materialized = MaterializedStoryInstanceSpec {
            story_id: story_id.clone(),
            pack: frozen.frozen_ref(),
            bindings,
            characters,
            relationships: Vec::new(),
            facts: Vec::new(),
            rumors: Vec::new(),
            memories: Vec::new(),
            scene: CurrentScene {
                scene_key: pack.start.scene_key.clone(),
                location_key: pack.start.location_key.clone(),
                time: pack.start.time.clone(),
                description: pack.start.description.clone(),
                present_character_ids,
            },
            opening: crate::domain::asset::validation::BoundedText::try_new(opening, "opening", 4096)
                .map_err(|_| StoryInstantiationError::LimitExceeded { limit: "opening" })?,
            narrative_state: NarrativeRuntimeState::initial(),
            created_at_ms: spec.created_at_ms,
        };
        let info = self
            .store
            .create_story_instance(&materialized)
            .await
            .map_err(StoryInstantiationError::Store)?;
        Ok(info)
    }
}

#[allow(dead_code)]
pub(crate) fn _instance_factory_anchor(
    _: &WorldFact,
    _: &SharedRumor,
    _: &MemoryEntry,
    _: &RelationshipState,
    _: &KnowledgeSource,
    _: &Sha256Digest,
) {
}
