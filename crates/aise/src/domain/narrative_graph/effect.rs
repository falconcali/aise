use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::ids::{CanonicalEventKey, LocationKey, NarrativeNodeKey, StoryRoleKey};
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::CharacterId;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::num::NonZeroU32;
use std::sync::Arc;

#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NarrativeEffectId(Arc<str>);

impl NarrativeEffectId {
    pub fn try_new(value: impl Into<String>) -> Result<Self, &'static str> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err("narrative effect id must not be empty");
        }
        Ok(Self(Arc::from(value)))
    }

    pub fn for_transition(
        source_node: &NarrativeNodeKey,
        transition: NarrativeTransitionKind,
        source_graph_revision: u64,
        effect_index: u32,
    ) -> Self {
        Self(Arc::from(format!(
            "narrative-effect:{source_node}:{}:{source_graph_revision}:{effect_index}",
            transition.as_str()
        )))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for NarrativeEffectId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for NarrativeEffectId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::try_new(value).map_err(serde::de::Error::custom)
    }
}

impl fmt::Display for NarrativeEffectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for NarrativeEffectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("NarrativeEffectId").field(&self.0).finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NarrativeTransitionKind {
    Activate,
    Complete,
    Skip,
}

impl NarrativeTransitionKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Activate => "activate",
            Self::Complete => "complete",
            Self::Skip => "skip",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum NarrativeEffectDefinition {
    WorldEvent(WorldEventIntentDefinition),
    CharacterImpulse(CharacterImpulseDefinition),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldEventIntentDefinition {
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
pub struct WorldEventIntent {
    pub effect_id: NarrativeEffectId,
    pub source_node: NarrativeNodeKey,
    pub event_key: CanonicalEventKey,
    pub category: BoundedText,
    pub participants: Vec<KnowledgeEntity>,
    pub location: Option<LocationKey>,
    pub description: BoundedText,
}

#[derive(Debug, Clone, Serialize)]
pub struct CharacterImpulse {
    pub effect_id: NarrativeEffectId,
    pub source_node: NarrativeNodeKey,
    pub target_role_key: StoryRoleKey,
    pub target_character_id: CharacterId,
    pub goal: BoundedText,
    pub reason: Option<BoundedText>,
    pub emotion: Option<BoundedText>,
    pub urgency: ImpulseUrgency,
    pub expires_after_turn: Option<u64>,
}
