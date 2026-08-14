use crate::domain::asset::character_card::CharacterCard;
use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::frozen_ref::FrozenStoryPackRef;
use crate::domain::asset::ids::{CanonicalEventKey, FactKey, Sha256Digest, StoryRoleKey, TopicKey};
use crate::domain::asset::story_pack::{StoryProfile, StoryRole};
use crate::domain::asset::validation::ScalarValue;
use crate::domain::asset::world_book::TopicDefinition;
use crate::domain::ids::{CharacterId, StoryId, StoryRevision};
use crate::domain::narrative::StoryContinuity;
use crate::domain::narrative_graph::definition::NarrativeGraphDefinition;
use crate::domain::narrative_graph::state::NarrativeRuntimeState;
use crate::domain::story_instance::binding::RoleBinding;
use crate::domain::story_instance::constraint::ActiveStoryConstraint;
use crate::domain::story_instance::state::{CharacterInstanceState, CurrentScene, InstanceSettings, RelationshipState};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeSnapshotRef {
    pub story_id: StoryId,
    pub pack_digest: Sha256Digest,
    pub base_revision: StoryRevision,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NarrativeConditionStateView {
    pub occurred_event_keys: BTreeSet<CanonicalEventKey>,
    pub player_action_event_keys: BTreeSet<CanonicalEventKey>,
    pub fact_values: BTreeMap<FactKey, ScalarValue>,
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum StorySnapshotError {
    #[error("story snapshot is inconsistent: {code}")]
    Inconsistent { code: &'static str },
}

#[derive(Debug, Clone)]
pub struct StoryReadSnapshot {
    story_id: StoryId,
    base_revision: StoryRevision,
    pack: FrozenStoryPackRef,
    story_profile: StoryProfile,
    instance_settings: InstanceSettings,
    role_definitions: BTreeMap<StoryRoleKey, StoryRole>,
    role_bindings: BTreeMap<StoryRoleKey, RoleBinding>,
    character_cards: BTreeMap<CharacterId, CharacterCard>,
    character_states: BTreeMap<CharacterId, CharacterInstanceState>,
    current_scene: CurrentScene,
    relationships: Vec<RelationshipState>,
    narrative_definition: NarrativeGraphDefinition,
    narrative_state: NarrativeRuntimeState,
    condition_state: NarrativeConditionStateView,
    story_continuity: StoryContinuity,
    active_constraints: Vec<ActiveStoryConstraint>,
    entity_catalog: Vec<KnowledgeEntity>,
    topic_dictionary: BTreeMap<TopicKey, TopicDefinition>,
    knowledge_snapshot: KnowledgeSnapshotRef,
}

#[derive(Debug, Clone)]
pub struct StoryReadSnapshotParts {
    pub story_id: StoryId,
    pub base_revision: StoryRevision,
    pub pack: FrozenStoryPackRef,
    pub story_profile: StoryProfile,
    pub instance_settings: InstanceSettings,
    pub role_definitions: BTreeMap<StoryRoleKey, StoryRole>,
    pub role_bindings: BTreeMap<StoryRoleKey, RoleBinding>,
    pub character_cards: BTreeMap<CharacterId, CharacterCard>,
    pub character_states: BTreeMap<CharacterId, CharacterInstanceState>,
    pub current_scene: CurrentScene,
    pub relationships: Vec<RelationshipState>,
    pub narrative_definition: NarrativeGraphDefinition,
    pub narrative_state: NarrativeRuntimeState,
    pub condition_state: NarrativeConditionStateView,
    pub story_continuity: StoryContinuity,
    pub active_constraints: Vec<ActiveStoryConstraint>,
    pub entity_catalog: Vec<KnowledgeEntity>,
    pub topic_dictionary: BTreeMap<TopicKey, TopicDefinition>,
    pub knowledge_snapshot: KnowledgeSnapshotRef,
}

impl StoryReadSnapshot {
    pub fn try_from_parts(parts: StoryReadSnapshotParts) -> Result<Self, StorySnapshotError> {
        let StoryReadSnapshotParts {
            story_id,
            base_revision,
            pack,
            story_profile,
            instance_settings,
            role_definitions,
            role_bindings,
            character_cards,
            character_states,
            current_scene,
            relationships,
            narrative_definition,
            narrative_state,
            condition_state,
            story_continuity,
            active_constraints,
            entity_catalog,
            topic_dictionary,
            knowledge_snapshot,
        } = parts;
        if knowledge_snapshot.story_id != story_id {
            return Err(StorySnapshotError::Inconsistent {
                code: "knowledge_story_id_mismatch",
            });
        }
        if knowledge_snapshot.pack_digest != pack.digest {
            return Err(StorySnapshotError::Inconsistent {
                code: "knowledge_pack_digest_mismatch",
            });
        }
        if knowledge_snapshot.base_revision != base_revision {
            return Err(StorySnapshotError::Inconsistent {
                code: "knowledge_base_revision_mismatch",
            });
        }
        if role_bindings.values().filter(|binding| binding.is_player_controlled()).count() != 1 {
            return inconsistent("player_binding_count");
        }
        if role_definitions.keys().ne(role_bindings.keys()) {
            return inconsistent("role_binding_set_mismatch");
        }
        let binding_characters = role_bindings
            .values()
            .map(|binding| binding.character_id.clone())
            .collect::<BTreeSet<_>>();
        if binding_characters.len() != role_bindings.len()
            || binding_characters.iter().ne(character_states.keys())
            || binding_characters.iter().ne(character_cards.keys())
        {
            return inconsistent("character_set_mismatch");
        }
        for (role_key, binding) in &role_bindings {
            let state = character_states
                .get(&binding.character_id)
                .ok_or(StorySnapshotError::Inconsistent {
                    code: "character_state_missing",
                })?;
            if &binding.role_key != role_key
                || &state.role_key != role_key
                || state.character_id != binding.character_id
                || binding.character_asset.character_key != character_cards[&binding.character_id].character_key
            {
                return inconsistent("binding_character_mismatch");
            }
        }
        validate_sorted_unique(&current_scene.present_character_ids, "scene_character_order")?;
        if current_scene
            .present_character_ids
            .iter()
            .any(|id| !character_states.contains_key(id))
        {
            return inconsistent("scene_character_missing");
        }
        let mut relationship_keys = BTreeSet::new();
        for relationship in &relationships {
            if !character_states.contains_key(&relationship.source_character_id)
                || !character_states.contains_key(&relationship.target_character_id)
                || !relationship_keys.insert(relationship.key())
            {
                return inconsistent("relationship_invalid");
            }
        }
        if narrative_state
            .node_states
            .keys()
            .chain(narrative_state.activation_turns.keys())
            .any(|key| !narrative_definition.nodes.contains_key(key))
        {
            return inconsistent("narrative_state_reference_invalid");
        }
        validate_sorted_unique(&entity_catalog, "entity_catalog_order")?;
        for entity in &entity_catalog {
            match entity {
                KnowledgeEntity::Role(key) if !role_definitions.contains_key(key) => {
                    return inconsistent("entity_role_missing");
                }
                KnowledgeEntity::Character(id) if !character_states.contains_key(id) => {
                    return inconsistent("entity_character_missing");
                }
                KnowledgeEntity::NarrativeNode(key) if !narrative_definition.nodes.contains_key(key) => {
                    return inconsistent("entity_narrative_node_missing");
                }
                _ => {}
            }
        }
        Ok(Self {
            story_id,
            base_revision,
            pack,
            story_profile,
            instance_settings,
            role_definitions,
            role_bindings,
            character_cards,
            character_states,
            current_scene,
            relationships,
            narrative_definition,
            narrative_state,
            condition_state,
            story_continuity,
            active_constraints,
            entity_catalog,
            topic_dictionary,
            knowledge_snapshot,
        })
    }

    pub fn story_id(&self) -> &StoryId {
        &self.story_id
    }

    pub fn base_revision(&self) -> StoryRevision {
        self.base_revision
    }

    pub fn pack(&self) -> &FrozenStoryPackRef {
        &self.pack
    }

    pub fn story_profile(&self) -> &StoryProfile {
        &self.story_profile
    }

    pub fn instance_settings(&self) -> &InstanceSettings {
        &self.instance_settings
    }

    pub fn role_definitions(&self) -> &BTreeMap<StoryRoleKey, StoryRole> {
        &self.role_definitions
    }

    pub fn role_binding(&self, key: &StoryRoleKey) -> Option<&RoleBinding> {
        self.role_bindings.get(key)
    }

    pub fn role_bindings(&self) -> &BTreeMap<StoryRoleKey, RoleBinding> {
        &self.role_bindings
    }

    pub fn character_cards(&self) -> &BTreeMap<CharacterId, CharacterCard> {
        &self.character_cards
    }

    pub fn character_states(&self) -> &BTreeMap<CharacterId, CharacterInstanceState> {
        &self.character_states
    }

    pub fn current_scene(&self) -> &CurrentScene {
        &self.current_scene
    }

    pub fn relationships(&self) -> &[RelationshipState] {
        &self.relationships
    }

    pub fn narrative_definition(&self) -> &NarrativeGraphDefinition {
        &self.narrative_definition
    }

    pub fn narrative_state(&self) -> &NarrativeRuntimeState {
        &self.narrative_state
    }

    pub fn condition_state(&self) -> &NarrativeConditionStateView {
        &self.condition_state
    }

    pub fn story_continuity(&self) -> &StoryContinuity {
        &self.story_continuity
    }

    pub fn active_constraints(&self) -> &[ActiveStoryConstraint] {
        &self.active_constraints
    }

    pub fn entity_catalog(&self) -> &[KnowledgeEntity] {
        &self.entity_catalog
    }

    pub fn topic_dictionary(&self) -> &BTreeMap<TopicKey, TopicDefinition> {
        &self.topic_dictionary
    }

    pub fn knowledge_snapshot(&self) -> &KnowledgeSnapshotRef {
        &self.knowledge_snapshot
    }

    pub fn graph_revision(&self) -> u64 {
        self.narrative_state.graph_revision
    }
}

fn validate_sorted_unique<T: Ord>(values: &[T], code: &'static str) -> Result<(), StorySnapshotError> {
    if values.windows(2).any(|pair| pair[0] >= pair[1]) {
        return inconsistent(code);
    }
    Ok(())
}

fn inconsistent<T>(code: &'static str) -> Result<T, StorySnapshotError> {
    Err(StorySnapshotError::Inconsistent { code })
}
