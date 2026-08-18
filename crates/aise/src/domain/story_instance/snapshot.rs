use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::frozen_ref::FrozenStoryPackRef;
use crate::domain::asset::ids::{FactKey, Sha256Digest, TopicKey};
use crate::domain::asset::story_pack::StoryProfile;
use crate::domain::asset::validation::{BoundedText, ScalarValue};
use crate::domain::asset::world_book::TopicDefinition;
use crate::domain::ids::{RoleId, RoleIdHighWater, StoryId, StoryRevision};
use crate::domain::knowledge::KnowledgeIdHighWater;
use crate::domain::narrative::StoryContinuity;
use crate::domain::narrative_graph::definition::NarrativeGraphDefinition;
use crate::domain::narrative_graph::state::NarrativeRuntimeState;
use crate::domain::story_instance::constraint::ActiveStoryConstraint;
use crate::domain::story_instance::role::StoryRoleView;
use crate::domain::story_instance::state::{InstanceSettings, RelationshipState};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeSnapshotRef {
    pub story_id: StoryId,
    pub pack_digest: Sha256Digest,
    pub base_revision: StoryRevision,
    pub knowledge_id_high_water: KnowledgeIdHighWater,
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
    story_title: BoundedText,
    story_profile: StoryProfile,
    instance_settings: InstanceSettings,
    roles: BTreeMap<RoleId, StoryRoleView>,
    player_role_id: RoleId,
    relationships: Vec<RelationshipState>,
    narrative_definition: NarrativeGraphDefinition,
    narrative_state: NarrativeRuntimeState,
    fact_values: BTreeMap<FactKey, ScalarValue>,
    story_continuity: StoryContinuity,
    active_constraints: Vec<ActiveStoryConstraint>,
    entity_catalog: Vec<KnowledgeEntity>,
    topic_dictionary: BTreeMap<TopicKey, TopicDefinition>,
    knowledge_snapshot: KnowledgeSnapshotRef,
    role_id_high_water: RoleIdHighWater,
}

#[derive(Debug, Clone)]
pub struct StoryReadSnapshotParts {
    pub story_id: StoryId,
    pub base_revision: StoryRevision,
    pub pack: FrozenStoryPackRef,
    pub story_title: BoundedText,
    pub story_profile: StoryProfile,
    pub instance_settings: InstanceSettings,
    pub roles: BTreeMap<RoleId, StoryRoleView>,
    pub relationships: Vec<RelationshipState>,
    pub narrative_definition: NarrativeGraphDefinition,
    pub narrative_state: NarrativeRuntimeState,
    pub fact_values: BTreeMap<FactKey, ScalarValue>,
    pub story_continuity: StoryContinuity,
    pub active_constraints: Vec<ActiveStoryConstraint>,
    pub entity_catalog: Vec<KnowledgeEntity>,
    pub topic_dictionary: BTreeMap<TopicKey, TopicDefinition>,
    pub knowledge_snapshot: KnowledgeSnapshotRef,
    pub role_id_high_water: RoleIdHighWater,
}

impl StoryReadSnapshot {
    pub fn try_from_parts(parts: StoryReadSnapshotParts) -> Result<Self, StorySnapshotError> {
        let StoryReadSnapshotParts {
            story_id,
            base_revision,
            pack,
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
            role_id_high_water,
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
        for (role_id, view) in &roles {
            if role_id != &view.role_id {
                return inconsistent("role_map_key_mismatch");
            }
        }
        let player_role_id = {
            let mut player_role_ids = roles
                .values()
                .filter(|role| role.is_player_controlled())
                .map(|role| role.role_id.clone());
            let first = player_role_ids.next();
            if first.is_none() || player_role_ids.next().is_some() {
                return inconsistent("player_role_count");
            }
            first.expect("checked above")
        };
        let mut relationship_keys = BTreeSet::new();
        for relationship in &relationships {
            if !roles.contains_key(&relationship.source_role_id) || !roles.contains_key(&relationship.target_role_id) {
                return inconsistent("relationship_role_missing");
            }
            if !relationship_keys.insert(relationship.key()) {
                return inconsistent("relationship_duplicate");
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
        if narrative_state
            .pending_effects
            .values()
            .any(|pending| !narrative_definition.nodes.contains_key(&pending.source_node))
        {
            return inconsistent("narrative_pending_effect_reference_invalid");
        }
        validate_sorted_unique(&entity_catalog, "entity_catalog_order")?;
        for entity in &entity_catalog {
            if let KnowledgeEntity::Role(role_id) = entity {
                if !roles.contains_key(role_id) {
                    return inconsistent("entity_role_missing");
                }
            }
            if let KnowledgeEntity::NarrativeNode(key) = entity {
                if !narrative_definition.nodes.contains_key(key) {
                    return inconsistent("entity_narrative_node_missing");
                }
            }
        }
        Ok(Self {
            story_id,
            base_revision,
            pack,
            story_title,
            story_profile,
            instance_settings,
            roles,
            player_role_id,
            relationships,
            narrative_definition,
            narrative_state,
            fact_values,
            story_continuity,
            active_constraints,
            entity_catalog,
            topic_dictionary,
            knowledge_snapshot,
            role_id_high_water,
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

    pub fn story_title(&self) -> &BoundedText {
        &self.story_title
    }

    pub fn story_profile(&self) -> &StoryProfile {
        &self.story_profile
    }

    pub fn instance_settings(&self) -> &InstanceSettings {
        &self.instance_settings
    }

    pub fn roles(&self) -> &BTreeMap<RoleId, StoryRoleView> {
        &self.roles
    }

    pub fn role(&self, role_id: &RoleId) -> Option<&StoryRoleView> {
        self.roles.get(role_id)
    }

    pub fn player_role_id(&self) -> &RoleId {
        &self.player_role_id
    }

    pub fn player_role(&self) -> &StoryRoleView {
        self.roles
            .get(&self.player_role_id)
            .expect("player role id is validated at construction")
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

    pub fn fact_values(&self) -> &BTreeMap<FactKey, ScalarValue> {
        &self.fact_values
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

    pub fn knowledge_id_high_water(&self) -> KnowledgeIdHighWater {
        self.knowledge_snapshot.knowledge_id_high_water
    }

    pub fn role_id_high_water(&self) -> RoleIdHighWater {
        self.role_id_high_water
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
