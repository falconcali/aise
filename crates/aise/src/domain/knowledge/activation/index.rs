use super::contracts::{ActivationEntryMetadata, ActivationMacroValues, ActivationPatternKind};
use super::engine::ActivationError;
use super::rule::{
    ActivationPattern, ActivationRuleLimits, ActivationRuleValidationError, SecondaryLogic, compile_activation_regex,
    normalize_activation_literal,
};
use super::scan::{ScanFragment, ScanFragmentId, ScanFragmentKind};
use crate::domain::asset::ids::Sha256Digest;
use crate::domain::knowledge::KnowledgeSourceId;
use regex::Regex;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::Arc;

pub const MATCHER_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActivationIndexLimits {
    pub max_entries: usize,
    pub max_overlay_entries: usize,
    pub max_tombstones: usize,
    pub max_literal_patterns: usize,
    pub max_regex_patterns: usize,
    pub max_compiled_bytes: usize,
    pub max_regex_program_bytes: usize,
    pub max_macro_expansion_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FrozenPackIndexKey {
    pub pack_digest: Sha256Digest,
    pub macro_digest: Sha256Digest,
    pub matcher_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FragmentPatternMatch {
    pub source_id: KnowledgeSourceId,
    pub pattern_kind: ActivationPatternKind,
    pub pattern_ordinal: u16,
    pub fragment_kind: ScanFragmentKind,
    pub recency_depth: u16,
    pub stable_fragment_order: u32,
    pub match_count: u16,
    pub group_score_contribution: u16,
}

#[derive(Debug)]
pub struct IndexedActivationPattern {
    source_id: KnowledgeSourceId,
    ordinal: u16,
    kind: ActivationPatternKind,
    group_score_contribution: u16,
    matcher: CompiledMatcher,
}

impl IndexedActivationPattern {
    pub fn source_id(&self) -> &KnowledgeSourceId {
        &self.source_id
    }

    pub fn kind(&self) -> ActivationPatternKind {
        self.kind
    }

    fn projection(&self, fragment: &ScanFragment, match_count: usize) -> FragmentPatternMatch {
        FragmentPatternMatch {
            source_id: self.source_id.clone(),
            pattern_kind: self.kind,
            pattern_ordinal: self.ordinal,
            fragment_kind: fragment.kind,
            recency_depth: fragment.recency_depth,
            stable_fragment_order: fragment.stable_order,
            match_count: u16::try_from(match_count).unwrap_or(u16::MAX),
            group_score_contribution: self.group_score_contribution,
        }
    }
}

#[derive(Debug)]
enum CompiledMatcher {
    Literal {
        needle: String,
        case_sensitive: bool,
        whole_words: bool,
    },
    Regex(Box<Regex>),
}

#[derive(Debug, Default)]
pub struct FrozenLiteralIndex {
    patterns: Vec<IndexedActivationPattern>,
    compiled_bytes: usize,
}

impl FrozenLiteralIndex {
    pub fn len(&self) -> usize {
        self.patterns.len()
    }

    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    pub fn compiled_bytes(&self) -> usize {
        self.compiled_bytes
    }

    pub fn match_fragment(&self, fragment: &ScanFragment, out: &mut Vec<FragmentPatternMatch>) {
        if self.patterns.is_empty() {
            return;
        }
        let sensitive = normalize_activation_literal(fragment.text.as_str(), true);
        let insensitive = normalize_activation_literal(fragment.text.as_str(), false);
        for pattern in &self.patterns {
            let CompiledMatcher::Literal {
                needle,
                case_sensitive,
                whole_words,
            } = &pattern.matcher
            else {
                continue;
            };
            let haystack = if *case_sensitive { &sensitive } else { &insensitive };
            let count = literal_count(haystack, needle, *whole_words);
            if count == 0 {
                continue;
            }
            out.push(pattern.projection(fragment, count));
        }
    }
}

#[derive(Debug, Default)]
pub struct FrozenRegexSet {
    patterns: Vec<IndexedActivationPattern>,
    compiled_bytes: usize,
}

impl FrozenRegexSet {
    pub fn len(&self) -> usize {
        self.patterns.len()
    }

    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    pub fn compiled_bytes(&self) -> usize {
        self.compiled_bytes
    }

    pub fn match_fragment(&self, fragment: &ScanFragment, out: &mut Vec<FragmentPatternMatch>) {
        for pattern in &self.patterns {
            let CompiledMatcher::Regex(regex) = &pattern.matcher else {
                continue;
            };
            let count = regex.find_iter(fragment.text.as_str()).count();
            if count == 0 {
                continue;
            }
            out.push(pattern.projection(fragment, count));
        }
    }
}

#[derive(Debug)]
pub struct FrozenPackIndex {
    pub key: FrozenPackIndexKey,
    pub literal_index: Arc<FrozenLiteralIndex>,
    pub regex_set: Arc<FrozenRegexSet>,
    pub entry_count: usize,
}

impl FrozenPackIndex {
    pub fn estimated_bytes(&self) -> usize {
        self.literal_index
            .compiled_bytes()
            .saturating_add(self.regex_set.compiled_bytes())
    }
}

#[derive(Debug, Default)]
pub struct CompiledActivationEntry {
    literal: Vec<IndexedActivationPattern>,
    regex: Vec<IndexedActivationPattern>,
    compiled_bytes: usize,
}

pub fn macro_digest(macros: &ActivationMacroValues) -> Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(b"aise.activation.macros.v1");
    hasher.update((macros.player_name.len() as u64).to_be_bytes());
    hasher.update(macros.player_name.as_bytes());
    hasher.update((macros.player_role_label.len() as u64).to_be_bytes());
    hasher.update(macros.player_role_label.as_bytes());
    Sha256Digest::from_bytes(hasher.finalize().into())
}

pub fn build_frozen_pack_index<'a>(
    key: FrozenPackIndexKey,
    entries: impl Iterator<Item = &'a ActivationEntryMetadata>,
    macros: &ActivationMacroValues,
    limits: ActivationIndexLimits,
    rule_limits: ActivationRuleLimits,
) -> Result<FrozenPackIndex, ActivationError> {
    validate_index_limits(limits)?;
    let mut literal = Vec::new();
    let mut regex = Vec::new();
    let mut compiled_bytes = 0usize;
    let mut entry_count = 0usize;
    for metadata in entries {
        entry_count = entry_count.saturating_add(1);
        if entry_count > limits.max_entries {
            return Err(ActivationError::WorkLimitExceeded {
                limit: "activation_index_entries",
            });
        }
        metadata.rule.validate(rule_limits).map_err(|error| match error {
            ActivationRuleValidationError::InvalidRegex => ActivationError::InvalidRegex,
            ActivationRuleValidationError::RegexProgramTooLarge => ActivationError::InvalidRegex,
            _ => ActivationError::InvalidRule {
                code: "activation_rule_rejected",
            },
        })?;
        let compiled = compile_entry(metadata, macros, limits)?;
        compiled_bytes = compiled_bytes.saturating_add(compiled.compiled_bytes);
        if compiled_bytes > limits.max_compiled_bytes {
            return Err(ActivationError::WorkLimitExceeded {
                limit: "activation_index_compiled_bytes",
            });
        }
        literal.extend(compiled.literal);
        regex.extend(compiled.regex);
        if literal.len() > limits.max_literal_patterns {
            return Err(ActivationError::WorkLimitExceeded {
                limit: "activation_index_literal_patterns",
            });
        }
        if regex.len() > limits.max_regex_patterns {
            return Err(ActivationError::WorkLimitExceeded {
                limit: "activation_index_regex_patterns",
            });
        }
    }
    let literal_bytes = literal
        .iter()
        .fold(0usize, |total, pattern| total.saturating_add(pattern_bytes(&pattern.matcher)));
    let regex_bytes = regex
        .iter()
        .fold(0usize, |total, pattern| total.saturating_add(pattern_bytes(&pattern.matcher)));
    Ok(FrozenPackIndex {
        key,
        literal_index: Arc::new(FrozenLiteralIndex {
            patterns: literal,
            compiled_bytes: literal_bytes,
        }),
        regex_set: Arc::new(FrozenRegexSet {
            patterns: regex,
            compiled_bytes: regex_bytes,
        }),
        entry_count,
    })
}

struct PatternCompileSpec<'a> {
    metadata: &'a ActivationEntryMetadata,
    literal_kind: ActivationPatternKind,
    regex_kind: ActivationPatternKind,
    positive_score: bool,
}

fn compile_entry(
    metadata: &ActivationEntryMetadata,
    macros: &ActivationMacroValues,
    limits: ActivationIndexLimits,
) -> Result<CompiledActivationEntry, ActivationError> {
    let mut compiled = CompiledActivationEntry::default();
    let rule = &metadata.rule;
    let secondary_positive = matches!(rule.match_rule.secondary_logic, SecondaryLogic::AndAny | SecondaryLogic::AndAll);
    compile_patterns(
        &mut compiled,
        &PatternCompileSpec {
            metadata,
            literal_kind: ActivationPatternKind::PrimaryLiteral,
            regex_kind: ActivationPatternKind::PrimaryRegex,
            positive_score: true,
        },
        &rule.match_rule.keys,
        macros,
        limits,
    )?;
    compile_patterns(
        &mut compiled,
        &PatternCompileSpec {
            metadata,
            literal_kind: ActivationPatternKind::SecondaryLiteral,
            regex_kind: ActivationPatternKind::SecondaryRegex,
            positive_score: secondary_positive,
        },
        &rule.match_rule.secondary_keys,
        macros,
        limits,
    )?;
    Ok(compiled)
}

fn compile_patterns(
    compiled: &mut CompiledActivationEntry,
    spec: &PatternCompileSpec<'_>,
    patterns: &[ActivationPattern],
    macros: &ActivationMacroValues,
    limits: ActivationIndexLimits,
) -> Result<(), ActivationError> {
    let metadata = spec.metadata;
    let rule = &metadata.rule;
    for (ordinal, pattern) in patterns.iter().enumerate() {
        let ActivationPattern::Literal(raw) = pattern;
        let ordinal = u16::try_from(ordinal).unwrap_or(u16::MAX);
        let group_score_contribution = u16::from(spec.positive_score);
        if let Some((expression, flags)) = regex_parts(raw.trim()) {
            let regex = compile_activation_regex(expression, flags, limits.max_regex_program_bytes)
                .map_err(|_| ActivationError::InvalidRegex)?;
            compiled.compiled_bytes = compiled.compiled_bytes.saturating_add(expression.len().saturating_mul(64));
            compiled.regex.push(IndexedActivationPattern {
                source_id: metadata.source_id.clone(),
                ordinal,
                kind: spec.regex_kind,
                group_score_contribution,
                matcher: CompiledMatcher::Regex(Box::new(regex)),
            });
            continue;
        }
        let expanded = expand_macros(raw.trim(), macros, limits.max_macro_expansion_bytes)?;
        let needle = normalize_activation_literal(&expanded, rule.match_rule.case_sensitive);
        if needle.is_empty() {
            return Err(ActivationError::InvalidRule {
                code: "empty_activation_pattern",
            });
        }
        compiled.compiled_bytes = compiled.compiled_bytes.saturating_add(needle.len());
        compiled.literal.push(IndexedActivationPattern {
            source_id: metadata.source_id.clone(),
            ordinal,
            kind: spec.literal_kind,
            group_score_contribution,
            matcher: CompiledMatcher::Literal {
                needle,
                case_sensitive: rule.match_rule.case_sensitive,
                whole_words: rule.match_rule.match_whole_words,
            },
        });
    }
    Ok(())
}

fn validate_index_limits(limits: ActivationIndexLimits) -> Result<(), ActivationError> {
    if limits.max_entries == 0
        || limits.max_overlay_entries == 0
        || limits.max_tombstones == 0
        || limits.max_literal_patterns == 0
        || limits.max_regex_patterns == 0
        || limits.max_compiled_bytes == 0
        || limits.max_regex_program_bytes == 0
        || limits.max_macro_expansion_bytes == 0
    {
        return Err(ActivationError::WorkLimitExceeded {
            limit: "activation_index_limits",
        });
    }
    Ok(())
}

fn expand_macros(value: &str, macros: &ActivationMacroValues, max_bytes: usize) -> Result<String, ActivationError> {
    let expanded = value
        .replace("{{player_name}}", &macros.player_name)
        .replace("{{player_role_label}}", &macros.player_role_label);
    if expanded.len() > max_bytes {
        return Err(ActivationError::WorkLimitExceeded {
            limit: "macro_expansion_bytes",
        });
    }
    Ok(expanded)
}

fn pattern_bytes(matcher: &CompiledMatcher) -> usize {
    match matcher {
        CompiledMatcher::Literal { needle, .. } => needle.len(),
        CompiledMatcher::Regex(regex) => regex.as_str().len().saturating_mul(64),
    }
}

pub fn regex_parts(value: &str) -> Option<(&str, &str)> {
    let rest = value.strip_prefix('/')?;
    let end = rest.rfind('/')?;
    Some((&rest[..end], &rest[end + 1..]))
}

fn literal_count(text: &str, needle: &str, whole_words: bool) -> usize {
    if needle.is_empty() {
        return 0;
    }
    text.match_indices(needle)
        .filter(|(offset, value)| {
            !whole_words || {
                let before = text[..*offset].chars().next_back();
                let after = text[*offset + value.len()..].chars().next();
                !before.is_some_and(|item| item.is_alphanumeric() || item == '_')
                    && !after.is_some_and(|item| item.is_alphanumeric() || item == '_')
            }
        })
        .count()
}

#[derive(Debug, Default)]
pub struct ActivationFragmentMatches {
    entries: BTreeMap<ScanFragmentId, Arc<Vec<FragmentPatternMatch>>>,
}

impl ActivationFragmentMatches {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, fragment_id: ScanFragmentId, matches: Arc<Vec<FragmentPatternMatch>>) {
        self.entries.insert(fragment_id, matches);
    }

    pub fn get(&self, fragment_id: &ScanFragmentId) -> Option<&Arc<Vec<FragmentPatternMatch>>> {
        self.entries.get(fragment_id)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

pub fn match_fragment(
    literal_index: &FrozenLiteralIndex,
    regex_set: &FrozenRegexSet,
    overlay_literal: &FrozenLiteralIndex,
    overlay_regex: &FrozenRegexSet,
    fragment: &ScanFragment,
) -> Vec<FragmentPatternMatch> {
    let mut matches = Vec::new();
    literal_index.match_fragment(fragment, &mut matches);
    regex_set.match_fragment(fragment, &mut matches);
    overlay_literal.match_fragment(fragment, &mut matches);
    overlay_regex.match_fragment(fragment, &mut matches);
    matches
}

pub fn summary_visible(kind: ScanFragmentKind, depth: u16, include_summary_at_max_depth: bool, max_depth: u16) -> bool {
    kind != ScanFragmentKind::StorySummary || (include_summary_at_max_depth && depth == max_depth)
}
