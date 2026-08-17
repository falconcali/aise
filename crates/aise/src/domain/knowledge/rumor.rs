use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::ids::{RumorKey, TopicKey};
use crate::domain::asset::validation::{BoundedText, ScalarValue};
use crate::domain::ids::{RoleId, RumorId};
use crate::domain::knowledge::hint::RetrievalHint;
use crate::domain::knowledge::query::KnowledgeSource;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedRumor {
    pub id: RumorId,
    pub key: Option<RumorKey>,
    pub content: BoundedText,
    pub claim: Option<Claim>,
    pub retrieval_hint: RetrievalHint,
    #[serde(default)]
    pub entities: Vec<KnowledgeEntity>,
    #[serde(default)]
    pub topics: Vec<TopicKey>,
    pub salience: u8,
    pub source_role_id: Option<RoleId>,
    pub truth_value: TruthValue,
    pub source: KnowledgeSource,
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
    pub subject: KnowledgeEntity,
    pub predicate: BoundedText,
    pub value: ScalarValue,
}
