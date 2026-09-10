use crate::domain::asset::character_card::AssetSpecVersion;
use crate::domain::asset::ids::{FactKey, RumorKey, SemanticVersion, WorldBookKey};
use crate::domain::asset::validation::{BoundedText, ScalarValue};
use crate::domain::knowledge::activation::KnowledgeActivationRule;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldBook {
    pub spec: WorldSpec,
    pub spec_version: AssetSpecVersion,
    pub world_book_key: WorldBookKey,
    pub meta: WorldBookMeta,
    #[serde(default)]
    pub facts: BTreeMap<FactKey, FactSeed>,
    #[serde(default)]
    pub rumors: BTreeMap<RumorKey, RumorSeed>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorldSpec {
    #[serde(rename = "aise_world_v5")]
    V5,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldBookMeta {
    pub name: BoundedText,
    pub version: SemanticVersion,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactSeed {
    pub proposition: Option<Proposition>,
    pub content: BoundedText,
    #[serde(default)]
    pub retrieval_hint: Option<BoundedText>,
    pub salience: u8,
    pub activation: KnowledgeActivationRule,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RumorSeed {
    pub claim: Option<Proposition>,
    pub content: BoundedText,
    #[serde(default)]
    pub retrieval_hint: Option<BoundedText>,
    pub salience: u8,
    pub activation: KnowledgeActivationRule,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposition {
    pub subject: BoundedText,
    pub predicate: BoundedText,
    pub value: ScalarValue,
}
