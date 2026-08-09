use crate::config::{AssetLimitsConfig, ContextPreparationConfig, TurnContentLimitsConfig};
use crate::core::token_estimator::estimate_text_tokens;
use crate::core::turn_data::retrieval::RetrievalSignals;
use crate::domain::asset::character_card::CharacterCard;
use crate::domain::asset::ids::{LocationKey, NarrativeNodeKey, Sha256Digest, StoryRoleKey};
use crate::domain::asset::story_pack::{StoryProfile, StoryRole};
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::CharacterId;
use crate::domain::narrative::{StoryContinuity, StoryContinuityLimits};
use crate::domain::narrative_graph::definition::NarrativeNodeState;
use crate::domain::story_instance::binding::RoleBinding;
use crate::domain::story_instance::constraint::ActiveStoryConstraint;
use crate::domain::story_instance::state::{CharacterInstanceState, CurrentScene, InstanceSettings};
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
    pub max_current_perceptions: usize,
    pub max_perception_bytes: usize,
    pub max_narrative_nodes: usize,
    pub max_condition_event_keys: usize,
    pub max_condition_fact_values: usize,
    pub max_constraints: usize,
    pub max_constraint_bytes: usize,
    pub max_topics: usize,
    pub max_topic_aliases_per_topic: usize,
    pub max_entity_catalog: usize,
    pub continuity: StoryContinuityLimits,
}

impl SnapshotLimits {
    pub fn from_config(
        content: &TurnContentLimitsConfig,
        context: &ContextPreparationConfig,
        assets: &AssetLimitsConfig,
    ) -> Self {
        Self {
            max_story_profile_bytes: content.max_story_profile_bytes,
            max_instance_settings: content.max_instance_settings,
            max_instance_setting_bytes: content.max_instance_setting_bytes,
            max_roles: assets.max_roles,
            max_characters: content.max_characters,
            max_character_bytes: content.max_character_bytes,
            max_scene_bytes: content.max_scene_bytes,
            max_scene_characters: context.max_scene_characters,
            max_relationships: context.max_relationships,
            max_current_perceptions: context.max_current_perceptions,
            max_perception_bytes: content.max_perception_bytes,
            max_narrative_nodes: assets.max_graph_nodes,
            max_condition_event_keys: context.max_condition_event_keys,
            max_condition_fact_values: context.max_condition_fact_values,
            max_constraints: content.max_constraints,
            max_constraint_bytes: content.max_constraint_bytes,
            max_topics: assets.max_topics,
            max_topic_aliases_per_topic: assets.max_topic_aliases_per_topic,
            max_entity_catalog: context.max_entity_catalog,
            continuity: StoryContinuityLimits {
                max_summary_bytes: content.max_summary_bytes,
                max_recent_segments: content.max_recent_segments,
                max_recent_segment_bytes: content.max_recent_segment_bytes,
                max_recent_segment_tokens: content.max_recent_segment_tokens,
            },
        }
    }
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
pub struct NarrativeStateView {
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
    pub character_index: Vec<CharacterIndexEntry>,
    pub story_continuity: StoryContinuity,
    pub active_story_constraints: Vec<ActiveStoryConstraint>,
    pub narrative_state_view: NarrativeStateView,
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
