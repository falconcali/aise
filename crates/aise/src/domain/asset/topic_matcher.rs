use crate::domain::asset::ids::TopicKey;
use crate::domain::asset::world_book::{
    TopicDefinition, TopicDictionaryError, normalize_topic_term, validate_topic_dictionary,
};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default)]
pub struct TopicMatcher;

impl TopicMatcher {
    pub fn validate_dictionary(dictionary: &BTreeMap<TopicKey, TopicDefinition>) -> Result<(), TopicDictionaryError> {
        validate_topic_dictionary(dictionary)
    }

    pub fn match_topics(&self, text: &str, dictionary: &BTreeMap<TopicKey, TopicDefinition>) -> Vec<TopicKey> {
        let haystack = normalize_topic_term(text);
        if haystack.is_empty() {
            return Vec::new();
        }
        let mut terms = Vec::new();
        for (topic, definition) in dictionary {
            terms.push((normalize_topic_term(definition.label.as_str()), topic.clone()));
            for alias in &definition.aliases {
                terms.push((normalize_topic_term(alias.as_str()), topic.clone()));
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
        let mut seen = std::collections::BTreeSet::new();
        for (term, topic) in terms {
            if term.is_empty() || seen.contains(&topic) {
                continue;
            }
            if term_matches(&haystack, &term) {
                seen.insert(topic.clone());
                matched.push(topic);
            }
        }
        matched.sort();
        matched
    }
}

pub fn term_matches(haystack: &str, term: &str) -> bool {
    if term.is_empty() {
        return false;
    }
    if term.chars().any(|ch| !ch.is_ascii_alphanumeric()) {
        return haystack.contains(term);
    }
    let bytes = haystack.as_bytes();
    let term_bytes = term.as_bytes();
    let mut start = 0usize;
    while start + term_bytes.len() <= bytes.len() {
        if &bytes[start..start + term_bytes.len()] == term_bytes {
            let before_ok = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
            let after = start + term_bytes.len();
            let after_ok = after == bytes.len() || !bytes[after].is_ascii_alphanumeric();
            if before_ok && after_ok {
                return true;
            }
        }
        start += 1;
    }
    false
}
