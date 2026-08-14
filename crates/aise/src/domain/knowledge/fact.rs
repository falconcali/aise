use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::ids::{FactKey, TopicKey};
use crate::domain::asset::validation::{BoundedText, ScalarValue};
use crate::domain::ids::FactId;
use crate::domain::knowledge::query::KnowledgeSource;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldFact {
    pub id: FactId,
    pub key: Option<FactKey>,
    pub text: BoundedText,
    pub proposition: Option<Proposition>,
    #[serde(default)]
    pub entities: Vec<KnowledgeEntity>,
    #[serde(default)]
    pub topics: Vec<TopicKey>,
    pub salience: u8,
    pub source: KnowledgeSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposition {
    pub subject: KnowledgeEntity,
    pub predicate: BoundedText,
    pub value: ScalarValue,
}
