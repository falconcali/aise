use crate::domain::asset::ids::FactKey;
use crate::domain::asset::validation::{BoundedText, ScalarValue};
use crate::domain::ids::{FactId, StoryRevision};
use crate::domain::knowledge::query::KnowledgeSource;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactSource {
    Seed,
    CommittedTurn,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldFact {
    pub id: FactId,
    pub key: Option<FactKey>,
    pub text: BoundedText,
    pub source: KnowledgeSource,
    pub story_revision: StoryRevision,
    pub proposition: Option<Proposition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposition {
    pub subject: crate::domain::asset::world_book::EntityRef,
    pub predicate: BoundedText,
    pub value: ScalarValue,
}
