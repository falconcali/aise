use crate::context::activation::scan_buffer::{ActivationScanBuffer, ScanFragmentKind};
use crate::domain::knowledge::activation::{ActivationPattern, normalize_activation_literal};
use regex::RegexBuilder;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationMatch {
    pub pattern_ordinal: u16,
    pub fragment_kind: ScanFragmentKind,
    pub recency_depth: u16,
    pub stable_fragment_order: u32,
    pub match_count: u16,
}

#[derive(Debug, Clone, Copy)]
pub struct ActivationMatcher {
    pub case_sensitive: bool,
    pub match_whole_words: bool,
}

impl ActivationMatcher {
    pub fn find(
        &self,
        buffer: &ActivationScanBuffer,
        patterns: &[ActivationPattern],
    ) -> Result<Vec<ActivationMatch>, ActivationMatcherError> {
        let mut matches = Vec::new();
        for (pattern_ordinal, pattern) in patterns.iter().enumerate() {
            for fragment in buffer.fragments() {
                let count = match pattern {
                    ActivationPattern::Literal(value) if is_regex(value) => regex_count(value, fragment.text.as_str())?,
                    ActivationPattern::Literal(value) => literal_count(
                        &normalize_activation_literal(value, self.case_sensitive),
                        fragment.text.as_str(),
                        self.case_sensitive,
                        self.match_whole_words,
                    ),
                };
                if count > 0 {
                    matches.push(ActivationMatch {
                        pattern_ordinal: u16::try_from(pattern_ordinal)
                            .map_err(|_| ActivationMatcherError::PatternLimit)?,
                        fragment_kind: fragment.kind,
                        recency_depth: fragment.recency_depth,
                        stable_fragment_order: fragment.stable_order,
                        match_count: count.min(u16::MAX as usize) as u16,
                    });
                }
            }
        }
        Ok(matches)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ActivationMatcherError {
    #[error("activation pattern ordinal exceeds the supported range")]
    PatternLimit,
    #[error("activation regex is invalid")]
    InvalidRegex,
}

fn is_regex(value: &str) -> bool {
    let Some(rest) = value.strip_prefix('/') else {
        return false;
    };
    rest.rfind('/').is_some_and(|index| index > 0)
}

fn regex_count(value: &str, text: &str) -> Result<usize, ActivationMatcherError> {
    let rest = value.strip_prefix('/').ok_or(ActivationMatcherError::InvalidRegex)?;
    let end = rest.rfind('/').ok_or(ActivationMatcherError::InvalidRegex)?;
    let expression = &rest[..end];
    let flags = &rest[end + 1..];
    let regex = RegexBuilder::new(expression)
        .case_insensitive(flags.contains('i'))
        .multi_line(flags.contains('m'))
        .dot_matches_new_line(flags.contains('s'))
        .unicode(true)
        .build()
        .map_err(|_| ActivationMatcherError::InvalidRegex)?;
    Ok(regex.find_iter(text).count())
}

fn literal_count(pattern: &str, text: &str, case_sensitive: bool, whole_words: bool) -> usize {
    if pattern.is_empty() {
        return 0;
    }
    let haystack = normalize_activation_literal(text, case_sensitive);
    let mut offset = 0usize;
    let mut count = 0usize;
    while let Some(relative) = haystack[offset..].find(pattern) {
        let start = offset + relative;
        let end = start + pattern.len();
        if !whole_words || whole_word_boundary(&haystack, start, end) {
            count = count.saturating_add(1);
        }
        offset = end.max(start.saturating_add(1));
        if offset >= haystack.len() {
            break;
        }
    }
    count
}

fn whole_word_boundary(text: &str, start: usize, end: usize) -> bool {
    let preceding = text[..start].chars().next_back();
    let following = text[end..].chars().next();
    preceding.is_none_or(|value| !value.is_alphanumeric() && value != '_')
        && following.is_none_or(|value| !value.is_alphanumeric() && value != '_')
}
