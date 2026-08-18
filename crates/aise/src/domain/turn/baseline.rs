use crate::domain::asset::character_card::CharacterProfile;
use crate::domain::asset::ids::{NarrativeNodeKey, Sha256Digest};
use crate::domain::asset::story_pack::StoryProfile;
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::RoleId;
use crate::domain::knowledge::{KnowledgeSourceId, RetrievalHint};
use crate::domain::narrative::{StoryContinuity, StoryContinuityLimits};
use crate::domain::narrative_graph::condition::NarrativeNodeState;
use crate::domain::story_instance::constraint::ActiveStoryConstraint;
use crate::domain::story_instance::role::{RoleController, StoryRoleState, StoryRoleView};
use crate::domain::story_instance::state::InstanceSettings;
use crate::domain::text::estimate_text_tokens;
use crate::domain::turn::retrieval::RetrievalSignals;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy)]
pub struct SnapshotLimits {
    pub max_story_profile_bytes: usize,
    pub max_instance_settings: usize,
    pub max_instance_setting_bytes: usize,
    pub max_roles: usize,
    pub max_role_bytes: usize,
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
pub struct RoleContextView {
    pub role_id: RoleId,
    pub role_label: BoundedText,
    pub narrative_function: BoundedText,
    pub background: Option<BoundedText>,
    pub profile: CharacterProfile,
    pub state: StoryRoleState,
    pub controller: RoleController,
}

impl From<&StoryRoleView> for RoleContextView {
    fn from(role: &StoryRoleView) -> Self {
        Self {
            role_id: role.role_id.clone(),
            role_label: role.role_label.clone(),
            narrative_function: role.narrative_function.clone(),
            background: role.background.clone(),
            profile: role.effective_profile.clone(),
            state: role.state.clone(),
            controller: role.controller.clone(),
        }
    }
}

impl RoleContextView {
    pub fn is_player_controlled(&self) -> bool {
        matches!(self.controller, RoleController::Player(_))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RoleIndexEntry {
    pub role_id: RoleId,
    pub retrieval_hint: BoundedText,
}

#[derive(Debug, Clone, Serialize)]
pub struct RelevantWorldKnowledgeItem {
    pub source_id: KnowledgeSourceId,
    pub content: BoundedText,
    pub source_priority: u8,
    pub salience: u8,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RelevantWorldKnowledge {
    pub facts: Vec<RelevantWorldKnowledgeItem>,
    pub rumors: Vec<RelevantWorldKnowledgeItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct KnowledgeIndexEntry {
    pub source_id: KnowledgeSourceId,
    pub retrieval_hint: RetrievalHint,
}

#[derive(Debug, Clone, Serialize)]
pub struct NarrativeGraphStateIndex {
    pub pack_digest: Sha256Digest,
    pub graph_revision: u64,
    pub node_states: BTreeMap<NarrativeNodeKey, NarrativeNodeState>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BaselineContext {
    pub story_title: BoundedText,
    pub story_profile: StoryProfile,
    pub instance_settings: InstanceSettings,
    pub player_role: RoleContextView,
    pub relevant_roles: Vec<RoleContextView>,
    pub relevant_world_knowledge: RelevantWorldKnowledge,
    pub role_index: Vec<RoleIndexEntry>,
    pub knowledge_index: Vec<KnowledgeIndexEntry>,
    pub story_continuity: StoryContinuity,
    pub active_story_constraints: Vec<ActiveStoryConstraint>,
    pub narrative_graph_state_index: NarrativeGraphStateIndex,
    pub retrieval_signals: RetrievalSignals,
}

impl BaselineContext {
    pub fn estimate_tokens(&self) -> u64 {
        let mut total = self.story_continuity.estimate_tokens();
        total = total.saturating_add(estimate_text_tokens(self.story_title.as_str()));
        total = total.saturating_add(estimate_text_tokens(self.player_role.profile.name.as_str()));
        for role in &self.relevant_roles {
            total = total.saturating_add(estimate_text_tokens(role.profile.name.as_str()));
        }
        for entry in self
            .relevant_world_knowledge
            .facts
            .iter()
            .chain(self.relevant_world_knowledge.rumors.iter())
        {
            total = total.saturating_add(estimate_text_tokens(entry.content.as_str()));
        }
        for entry in &self.role_index {
            total = total.saturating_add(estimate_text_tokens(entry.retrieval_hint.as_str()));
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
