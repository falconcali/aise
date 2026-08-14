use crate::domain::asset::character_card::CharacterCard;
use crate::domain::asset::ids::{LocationKey, NarrativeNodeKey, Sha256Digest, StoryRoleKey};
use crate::domain::asset::story_pack::{StoryProfile, StoryRole};
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::CharacterId;
use crate::domain::knowledge::{KnowledgeKind, KnowledgeSourceId};
use crate::domain::narrative::{StoryContinuity, StoryContinuityLimits};
use crate::domain::narrative_graph::condition::NarrativeNodeState;
use crate::domain::story_instance::binding::RoleBinding;
use crate::domain::story_instance::constraint::ActiveStoryConstraint;
use crate::domain::story_instance::state::{CharacterInstanceState, CurrentScene, InstanceSettings};
use crate::domain::text::estimate_text_tokens;
use crate::domain::turn::retrieval::RetrievalSignals;
use crate::domain::turn::{RetrievalIndexScope, RetrievalTargetId};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy)]
pub struct SnapshotLimits {
    pub max_story_profile_bytes: usize,
    pub max_instance_settings: usize,
    pub max_instance_setting_bytes: usize,
    pub max_roles: usize,
    pub max_characters: usize,
    pub max_character_bytes: usize,
    pub max_scene_bytes: usize,
    pub max_scene_characters: usize,
    pub max_relationships: usize,
    pub max_narrative_nodes: usize,
    pub max_condition_fact_values: usize,
    pub max_constraints: usize,
    pub max_constraint_bytes: usize,
    pub max_topics: usize,
    pub max_topic_aliases_per_topic: usize,
    pub max_entity_catalog: usize,
    pub continuity: StoryContinuityLimits,
}

#[derive(Debug, Clone, Serialize)]
pub struct CharacterView {
    pub character_id: CharacterId,
    pub role_key: StoryRoleKey,
    pub role: StoryRole,
    pub binding: RoleBinding,
    pub card: CharacterCard,
    pub state: CharacterInstanceState,
}

#[derive(Debug, Clone, Serialize)]
pub struct CharacterIndexEntry {
    pub character_id: CharacterId,
    pub role_key: StoryRoleKey,
    pub name: BoundedText,
    pub narrative_function: BoundedText,
    pub location_key: LocationKey,
    pub player_controlled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RelevantKnowledge {
    pub entry_id: KnowledgeSourceId,
    pub kind: KnowledgeKind,
    pub content: BoundedText,
    pub source_priority: u8,
    pub salience: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct KnowledgeEntryIndexEntry {
    pub target_id: RetrievalTargetId,
    pub entry_id: KnowledgeSourceId,
    pub kind: KnowledgeKind,
    pub retrieval_hint: BoundedText,
}

#[derive(Debug, Clone, Serialize)]
pub struct NarrativeGraphStateIndex {
    pub pack_digest: Sha256Digest,
    pub graph_revision: u64,
    pub node_states: BTreeMap<NarrativeNodeKey, NarrativeNodeState>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BaselineContext {
    pub story_profile: StoryProfile,
    pub instance_settings: InstanceSettings,
    pub player_character: CharacterView,
    pub current_scene: CurrentScene,
    pub scene_characters: Vec<CharacterView>,
    pub referenced_characters: Vec<CharacterView>,
    pub relevant_knowledge: Vec<RelevantKnowledge>,
    pub character_index_scope: RetrievalIndexScope,
    pub knowledge_entry_index_scope: RetrievalIndexScope,
    pub knowledge_entry_index: Vec<KnowledgeEntryIndexEntry>,
    pub character_index: Vec<CharacterIndexEntry>,
    pub story_continuity: StoryContinuity,
    pub active_story_constraints: Vec<ActiveStoryConstraint>,
    pub narrative_graph_state_index: NarrativeGraphStateIndex,
    pub retrieval_signals: RetrievalSignals,
}

impl BaselineContext {
    pub fn estimate_tokens(&self) -> u64 {
        let mut total = self.story_continuity.estimate_tokens();
        total = total.saturating_add(estimate_text_tokens(self.story_profile.premise.as_str()));
        total = total.saturating_add(estimate_text_tokens(self.current_scene.description.as_str()));
        total = total.saturating_add(estimate_text_tokens(self.player_character.card.meta.name.as_str()));
        for character in &self.scene_characters {
            total = total.saturating_add(estimate_text_tokens(character.card.meta.name.as_str()));
        }
        for character in &self.referenced_characters {
            total = total.saturating_add(estimate_text_tokens(character.card.meta.name.as_str()));
        }
        for entry in &self.relevant_knowledge {
            total = total.saturating_add(estimate_text_tokens(entry.content.as_str()));
        }
        for entry in &self.character_index {
            total = total.saturating_add(estimate_text_tokens(entry.name.as_str()));
        }
        for constraint in &self.active_story_constraints {
            let statement = match &constraint.requirement {
                crate::domain::asset::constraint::StoryConstraintRequirement::Require { statement }
                | crate::domain::asset::constraint::StoryConstraintRequirement::Forbid { statement } => {
                    statement.as_str()
                }
            };
            total = total.saturating_add(estimate_text_tokens(statement));
        }
        total.max(1)
    }
}
