use crate::domain::asset::character_card::CharacterCard;
use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::frozen_ref::FrozenStoryPackRef;
use crate::domain::asset::ids::{CanonicalEventKey, FactKey, Sha256Digest, StoryRoleKey, TopicKey};
use crate::domain::asset::story_pack::{StoryProfile, StoryRole};
use crate::domain::asset::validation::ScalarValue;
use crate::domain::asset::world_book::TopicDefinition;
use crate::domain::ids::{CharacterId, StoryId, StoryRevision};
use crate::domain::knowledge::query::CurrentPerception;
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
    current_perceptions: Vec<CurrentPerception>,
    narrative_definition: NarrativeGraphDefinition,
    narrative_state: NarrativeRuntimeState,
    condition_state: NarrativeConditionStateView,
    story_continuity: StoryContinuity,
    active_constraints: Vec<ActiveStoryConstraint>,
    entity_catalog: Vec<KnowledgeEntity>,
    topic_dictionary: BTreeMap<TopicKey, TopicDefinition>,
    knowledge_snapshot: KnowledgeSnapshotRef,
}

impl StoryReadSnapshot {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
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
        current_perceptions: Vec<CurrentPerception>,
        narrative_definition: NarrativeGraphDefinition,
        narrative_state: NarrativeRuntimeState,
        condition_state: NarrativeConditionStateView,
        story_continuity: StoryContinuity,
        active_constraints: Vec<ActiveStoryConstraint>,
        entity_catalog: Vec<KnowledgeEntity>,
        topic_dictionary: BTreeMap<TopicKey, TopicDefinition>,
        knowledge_snapshot: KnowledgeSnapshotRef,
    ) -> Result<Self, StorySnapshotError> {
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

    pub fn current_perceptions(&self) -> &[CurrentPerception] {
        &self.current_perceptions
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
