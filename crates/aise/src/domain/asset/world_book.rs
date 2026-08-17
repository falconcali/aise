use crate::domain::asset::character_card::AssetSpecVersion;
use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::ids::{FactKey, RumorKey, SemanticVersion, TopicKey, WorldBookKey};
use crate::domain::asset::validation::{BoundedText, ScalarValue};
use crate::domain::knowledge::RetrievalHint;
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
    pub topics: BTreeMap<TopicKey, TopicDefinition>,
    #[serde(default)]
    pub facts: BTreeMap<FactKey, FactSeed>,
    #[serde(default)]
    pub rumors: BTreeMap<RumorKey, RumorSeed>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorldSpec {
    #[serde(rename = "aise_world_v4")]
    V4,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldBookMeta {
    pub name: BoundedText,
    pub version: SemanticVersion,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TopicDefinition {
    pub label: BoundedText,
    #[serde(default)]
    pub aliases: Vec<BoundedText>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TopicDictionaryError {
    #[error("topic alias collision after normalization: {normalized}")]
    AliasCollision { normalized: String },
}

pub fn normalize_topic_term(value: &str) -> String {
    let lower = value.to_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut previous_space = false;
    for ch in lower.chars() {
        if ch.is_whitespace() {
            if !previous_space && !out.is_empty() {
                out.push(' ');
                previous_space = true;
            }
            continue;
        }
        previous_space = false;
        out.push(ch);
    }
    out.trim().to_owned()
}

pub fn validate_topic_dictionary(dictionary: &BTreeMap<TopicKey, TopicDefinition>) -> Result<(), TopicDictionaryError> {
    let mut seen = BTreeMap::<String, TopicKey>::new();
    for (topic, definition) in dictionary {
        let mut terms = vec![normalize_topic_term(definition.label.as_str())];
        for alias in &definition.aliases {
            terms.push(normalize_topic_term(alias.as_str()));
        }
        for term in terms {
            if term.is_empty() {
                continue;
            }
            if let Some(existing) = seen.get(&term) {
                if existing != topic {
                    return Err(TopicDictionaryError::AliasCollision { normalized: term });
                }
            } else {
                seen.insert(term, topic.clone());
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactSeed {
    pub proposition: Option<Proposition>,
    pub content: BoundedText,
    pub retrieval_hint: RetrievalHint,
    #[serde(default)]
    pub entities: Vec<KnowledgeEntity>,
    #[serde(default)]
    pub topics: Vec<TopicKey>,
    pub salience: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RumorSeed {
    pub claim: Option<Proposition>,
    pub content: BoundedText,
    pub retrieval_hint: RetrievalHint,
    #[serde(default)]
    pub entities: Vec<KnowledgeEntity>,
    #[serde(default)]
    pub topics: Vec<TopicKey>,
    pub salience: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposition {
    pub subject: KnowledgeEntity,
    pub predicate: BoundedText,
    pub value: ScalarValue,
}
