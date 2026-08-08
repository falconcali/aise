use crate::domain::asset::ids::{RumorId, StoryRoleKey, TopicKey};
use crate::domain::asset::validation::{BoundedText, ScalarValue};
use crate::domain::ids::{CharacterId, StoryRevision};
use crate::domain::knowledge::query::KnowledgeSource;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedRumor {
    pub id: RumorId,
    pub key: Option<crate::domain::asset::ids::RumorKey>,
    pub content: BoundedText,
    pub claim: Option<Claim>,
    pub source_role_key: Option<StoryRoleKey>,
    pub source_character_id: Option<CharacterId>,
    pub truth_value: TruthValue,
    pub source: KnowledgeSource,
    pub story_revision: StoryRevision,
    #[serde(default)]
    pub tags: Vec<TopicKey>,
    pub salience: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TruthValue {
    True,
    False,
    Unverified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Claim {
    pub subject: crate::domain::asset::world_book::EntityRef,
    pub predicate: BoundedText,
    pub value: ScalarValue,
}
