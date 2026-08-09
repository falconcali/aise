use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::ids::{CanonicalEventKey, LocationKey, NarrativeNodeKey, StoryRoleKey};
use crate::domain::asset::validation::BoundedText;
use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum NarrativeEffectDefinition {
    GlobalEvent(GlobalEventIntentDefinition),
    CharacterImpulse(CharacterImpulseDefinition),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlobalEventIntentDefinition {
    pub event_key: CanonicalEventKey,
    pub category: BoundedText,
    #[serde(default)]
    pub participants: Vec<KnowledgeEntity>,
    pub location: Option<LocationKey>,
    pub description: BoundedText,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterImpulseDefinition {
    pub target_role_key: StoryRoleKey,
    pub goal: BoundedText,
    pub reason: Option<BoundedText>,
    pub emotion: Option<BoundedText>,
    pub urgency: ImpulseUrgency,
    pub valid_for_turns: Option<NonZeroU32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImpulseUrgency {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlobalEventIntent {
    pub source_node: NarrativeNodeKey,
    pub event_key: CanonicalEventKey,
    pub category: BoundedText,
    pub participants: Vec<KnowledgeEntity>,
    pub location: Option<LocationKey>,
    pub description: BoundedText,
}

#[derive(Debug, Clone, Serialize)]
pub struct CharacterImpulse {
    pub source_node: NarrativeNodeKey,
    pub target_role_key: StoryRoleKey,
    pub target_character_id: crate::domain::ids::CharacterId,
    pub goal: BoundedText,
    pub reason: Option<BoundedText>,
    pub emotion: Option<BoundedText>,
    pub urgency: ImpulseUrgency,
    pub expires_after_turn: Option<u64>,
}
