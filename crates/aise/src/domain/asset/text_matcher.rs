use crate::domain::asset::ids::TopicKey;
use crate::domain::asset::validation::BoundedText;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Default)]
pub struct TextMatcher;

impl TextMatcher {
    pub fn validate_dictionary(dictionary: &BTreeMap<TopicKey, TopicDefinition>) -> Result<(), TopicDictionaryError> {
        let mut seen = BTreeMap::<String, TopicKey>::new();
        for (topic, definition) in dictionary {
            let terms = std::iter::once(&definition.label).chain(definition.aliases.iter());
            for term in terms {
                let normalized = normalize_match_text(term.as_str());
                if normalized.is_empty() {
                    continue;
                }
                if let Some(existing) = seen.get(&normalized) {
                    if existing != topic {
                        return Err(TopicDictionaryError::AliasCollision { normalized });
                    }
                } else {
                    seen.insert(normalized, topic.clone());
                }
            }
        }
        Ok(())
    }

    pub fn match_topics(&self, text: &str, dictionary: &BTreeMap<TopicKey, TopicDefinition>) -> Vec<TopicKey> {
        let haystack = normalize_match_text(text);
        if haystack.is_empty() {
            return Vec::new();
        }
        let mut terms = Vec::new();
        for (topic, definition) in dictionary {
            terms.push((normalize_match_text(definition.label.as_str()), topic.clone()));
            for alias in &definition.aliases {
                terms.push((normalize_match_text(alias.as_str()), topic.clone()));
            }
        }
        terms.sort_by(|left, right| {
            right
                .0
                .chars()
                .count()
                .cmp(&left.0.chars().count())
                .then_with(|| left.1.cmp(&right.1))
        });
        let mut matched = Vec::new();
        let mut seen = BTreeSet::new();
        for (term, topic) in terms {
            if !term.is_empty() && !seen.contains(&topic) && term_matches(&haystack, &term) {
                seen.insert(topic.clone());
                matched.push(topic);
            }
        }
        matched.sort();
        matched
    }
}

pub fn normalize_match_text(value: &str) -> String {
    let lower = value.to_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut previous_space = false;
    for ch in lower.chars() {
        if ch.is_whitespace() {
            if !previous_space && !out.is_empty() {
                out.push(' ');
                previous_space = true;
            }
        } else {
            previous_space = false;
            out.push(ch);
        }
    }
    out.trim().to_owned()
}

pub fn term_matches(haystack: &str, term: &str) -> bool {
    if term.is_empty() {
        return false;
    }
    if !term.is_ascii() {
        return haystack.contains(term);
    }
    haystack.match_indices(term).any(|(start, value)| {
        let end = start + value.len();
        let before = haystack[..start].chars().next_back();
        let after = haystack[end..].chars().next();
        before.is_none_or(|ch| !ch.is_ascii_alphanumeric()) && after.is_none_or(|ch| !ch.is_ascii_alphanumeric())
    })
}
