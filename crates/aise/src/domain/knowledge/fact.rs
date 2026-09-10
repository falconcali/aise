use crate::domain::asset::ids::FactKey;
use crate::domain::asset::validation::{BoundedText, ScalarValue};
use crate::domain::ids::FactId;
use crate::domain::knowledge::activation::{ActivationRuleVersion, KnowledgeActivationRule};
use crate::domain::knowledge::hint::RetrievalHint;
use crate::domain::knowledge::query::KnowledgeSource;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldFact {
    pub id: FactId,
    pub key: Option<FactKey>,
    pub text: BoundedText,
    pub proposition: Option<Proposition>,
    pub retrieval_hint: RetrievalHint,
    pub activation: KnowledgeActivationRule,
    pub activation_rule_version: ActivationRuleVersion,
    pub salience: u8,
    pub source: KnowledgeSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposition {
    pub subject: BoundedText,
    pub predicate: BoundedText,
    pub value: ScalarValue,
}
