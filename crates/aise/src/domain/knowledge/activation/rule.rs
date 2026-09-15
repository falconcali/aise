use crate::domain::asset::ids::Sha256Digest;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeActivationRule {
    #[serde(rename = "match")]
    pub match_rule: ActivationMatchRule,
    pub mode: ActivationMode,
    pub recursion: ActivationRecursionRule,
    pub selection: ActivationSelectionRule,
    pub timing: ActivationTimingRule,
    pub scope: ActivationScopeRule,
    pub budget_class: ActivationBudgetClass,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationMatchRule {
    #[serde(default)]
    pub keys: Vec<ActivationPattern>,
    #[serde(default)]
    pub secondary_keys: Vec<ActivationPattern>,
    #[serde(default)]
    pub secondary_logic: SecondaryLogic,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub match_whole_words: bool,
    pub scan_depth: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ActivationPattern {
    Literal(String),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecondaryLogic {
    #[default]
    AndAny,
    AndAll,
    NotAny,
    NotAll,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationMode {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub constant: bool,
    #[serde(default)]
    pub exact_target_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationRecursionRule {
    #[serde(default)]
    pub exclude_recursion: bool,
    #[serde(default)]
    pub prevent_recursion: bool,
    pub delay_until_recursion: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationSelectionRule {
    #[serde(default)]
    pub order: i32,
    #[serde(default = "default_probability")]
    pub probability: u8,
    #[serde(default)]
    pub groups: Vec<ActivationGroupKey>,
    #[serde(default)]
    pub group_override: bool,
    #[serde(default = "default_group_weight")]
    pub group_weight: u32,
    #[serde(default)]
    pub use_group_scoring: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationTimingRule {
    #[serde(default)]
    pub sticky_turns: u16,
    #[serde(default)]
    pub cooldown_turns: u16,
    #[serde(default)]
    pub delay_turns: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationScopeRule {
    #[serde(default)]
    pub generation_triggers: Vec<GenerationTrigger>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GenerationTrigger {
    Normal,
    Continue,
    Regenerate,
    Repair,
    DryRunPreview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationBudgetClass {
    Normal,
    Reserved,
    Mandatory,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ActivationRuleVersion(Sha256Digest);

impl ActivationRuleVersion {
    pub fn from_rule(rule: &KnowledgeActivationRule) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"aise.activation.rule.v1\x00");
        hash_patterns(&mut hasher, b"keys", &rule.match_rule.keys);
        hash_patterns(&mut hasher, b"secondary_keys", &rule.match_rule.secondary_keys);
        hash_field(
            &mut hasher,
            b"secondary_logic",
            secondary_logic_tag(rule.match_rule.secondary_logic),
        );
        hash_bool(&mut hasher, b"case_sensitive", rule.match_rule.case_sensitive);
        hash_bool(&mut hasher, b"match_whole_words", rule.match_rule.match_whole_words);
        hash_optional_u16(&mut hasher, b"scan_depth", rule.match_rule.scan_depth);
        hash_bool(&mut hasher, b"enabled", rule.mode.enabled);
        hash_bool(&mut hasher, b"constant", rule.mode.constant);
        hash_bool(&mut hasher, b"exact_target_only", rule.mode.exact_target_only);
        hash_bool(&mut hasher, b"exclude_recursion", rule.recursion.exclude_recursion);
        hash_bool(&mut hasher, b"prevent_recursion", rule.recursion.prevent_recursion);
        hash_optional_u16(&mut hasher, b"delay_until_recursion", rule.recursion.delay_until_recursion);
        hash_field(&mut hasher, b"order", &rule.selection.order.to_be_bytes());
        hash_field(&mut hasher, b"probability", &rule.selection.probability.to_be_bytes());
        hash_field(&mut hasher, b"group_count", &(rule.selection.groups.len() as u64).to_be_bytes());
        for group in &rule.selection.groups {
            hash_field(&mut hasher, b"group", group.as_str().as_bytes());
        }
        hash_bool(&mut hasher, b"group_override", rule.selection.group_override);
        hash_field(&mut hasher, b"group_weight", &rule.selection.group_weight.to_be_bytes());
        hash_bool(&mut hasher, b"use_group_scoring", rule.selection.use_group_scoring);
        hash_field(&mut hasher, b"sticky_turns", &rule.timing.sticky_turns.to_be_bytes());
        hash_field(&mut hasher, b"cooldown_turns", &rule.timing.cooldown_turns.to_be_bytes());
        hash_field(&mut hasher, b"delay_turns", &rule.timing.delay_turns.to_be_bytes());
        hash_field(
            &mut hasher,
            b"trigger_count",
            &(rule.scope.generation_triggers.len() as u64).to_be_bytes(),
        );
        for trigger in &rule.scope.generation_triggers {
            hash_field(&mut hasher, b"trigger", &[generation_trigger_tag(*trigger)]);
        }
        hash_field(&mut hasher, b"budget_class", &[budget_class_tag(rule.budget_class)]);
        Self(Sha256Digest::from_bytes(hasher.finalize().into()))
    }

    pub fn as_digest(&self) -> &Sha256Digest {
        &self.0
    }

    pub fn from_digest(digest: Sha256Digest) -> Self {
        Self(digest)
    }
}

fn hash_field(hasher: &mut Sha256, label: &[u8], value: &[u8]) {
    hasher.update(label);
    hasher.update(b"=");
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
    hasher.update(b";");
}

fn hash_bool(hasher: &mut Sha256, label: &[u8], value: bool) {
    hash_field(hasher, label, &[u8::from(value)]);
}

fn hash_optional_u16(hasher: &mut Sha256, label: &[u8], value: Option<u16>) {
    match value {
        Some(value) => hash_field(hasher, label, &value.to_be_bytes()),
        None => hash_field(hasher, label, b""),
    }
}

fn hash_patterns(hasher: &mut Sha256, label: &[u8], patterns: &[ActivationPattern]) {
    hash_field(hasher, label, &(patterns.len() as u64).to_be_bytes());
    for pattern in patterns {
        let ActivationPattern::Literal(value) = pattern;
        hash_field(hasher, label, value.as_bytes());
    }
}

fn secondary_logic_tag(logic: SecondaryLogic) -> &'static [u8] {
    match logic {
        SecondaryLogic::AndAny => b"and_any",
        SecondaryLogic::AndAll => b"and_all",
        SecondaryLogic::NotAny => b"not_any",
        SecondaryLogic::NotAll => b"not_all",
    }
}

fn generation_trigger_tag(trigger: GenerationTrigger) -> u8 {
    match trigger {
        GenerationTrigger::Normal => 0,
        GenerationTrigger::Continue => 1,
        GenerationTrigger::Regenerate => 2,
        GenerationTrigger::Repair => 3,
        GenerationTrigger::DryRunPreview => 4,
    }
}

fn budget_class_tag(class: ActivationBudgetClass) -> u8 {
    match class {
        ActivationBudgetClass::Normal => 0,
        ActivationBudgetClass::Reserved => 1,
        ActivationBudgetClass::Mandatory => 2,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ActivationGroupKey(Arc<str>);

impl ActivationGroupKey {
    pub const MAX_BYTES: usize = 128;

    pub fn try_new(value: impl Into<String>) -> Result<Self, ActivationRuleValidationError> {
        let value = value.into();
        if value.is_empty() || value.len() > Self::MAX_BYTES {
            return Err(ActivationRuleValidationError::InvalidGroupKey);
        }
        let mut previous_separator = true;
        for character in value.chars() {
            if character.is_ascii_lowercase() || character.is_ascii_digit() {
                previous_separator = false;
            } else if matches!(character, '.' | '_' | '-') && !previous_separator {
                previous_separator = true;
            } else {
                return Err(ActivationRuleValidationError::InvalidGroupKey);
            }
        }
        if previous_separator {
            return Err(ActivationRuleValidationError::InvalidGroupKey);
        }
        Ok(Self(Arc::from(value)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActivationRuleLimits {
    pub max_primary_patterns_per_entry: usize,
    pub max_secondary_patterns_per_entry: usize,
    pub max_pattern_bytes: usize,
    pub max_regex_program_bytes: usize,
    pub max_groups_per_entry: usize,
    pub max_group_key_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ActivationRuleValidationError {
    #[error("activation rule has no activation source")]
    MissingActivationSource,
    #[error("constant and exact target only are mutually exclusive")]
    ConflictingModes,
    #[error("activation probability is outside 0..=100")]
    InvalidProbability,
    #[error("activation group key is invalid")]
    InvalidGroupKey,
    #[error("activation rule has too many groups")]
    TooManyGroups,
    #[error("activation pattern is empty")]
    EmptyPattern,
    #[error("activation pattern is invalid")]
    InvalidPattern,
    #[error("activation regex is invalid")]
    InvalidRegex,
    #[error("activation macro is invalid")]
    InvalidMacro,
    #[error("activation rule limit must be positive")]
    InvalidLimit,
    #[error("activation rule has too many primary patterns")]
    TooManyPrimaryPatterns,
    #[error("activation rule has too many secondary patterns")]
    TooManySecondaryPatterns,
    #[error("activation pattern exceeds its byte budget")]
    PatternTooLong,
    #[error("activation regex program exceeds its byte budget")]
    RegexProgramTooLarge,
    #[error("activation group key exceeds its byte budget")]
    GroupKeyTooLong,
}

impl KnowledgeActivationRule {
    pub fn disabled() -> Self {
        Self {
            match_rule: ActivationMatchRule {
                keys: Vec::new(),
                secondary_keys: Vec::new(),
                secondary_logic: SecondaryLogic::AndAny,
                case_sensitive: false,
                match_whole_words: false,
                scan_depth: None,
            },
            mode: ActivationMode {
                enabled: false,
                constant: false,
                exact_target_only: false,
            },
            recursion: ActivationRecursionRule {
                exclude_recursion: false,
                prevent_recursion: false,
                delay_until_recursion: None,
            },
            selection: ActivationSelectionRule {
                order: 0,
                probability: 100,
                groups: Vec::new(),
                group_override: false,
                group_weight: 100,
                use_group_scoring: false,
            },
            timing: ActivationTimingRule {
                sticky_turns: 0,
                cooldown_turns: 0,
                delay_turns: 0,
            },
            scope: ActivationScopeRule {
                generation_triggers: Vec::new(),
            },
            budget_class: ActivationBudgetClass::Normal,
        }
    }

    pub fn validate(&self, limits: ActivationRuleLimits) -> Result<(), ActivationRuleValidationError> {
        self.validate_inner(limits, true)
    }

    pub(crate) fn validate_for_index(&self, limits: ActivationRuleLimits) -> Result<(), ActivationRuleValidationError> {
        self.validate_inner(limits, false)
    }

    fn validate_inner(
        &self,
        limits: ActivationRuleLimits,
        compile_regex: bool,
    ) -> Result<(), ActivationRuleValidationError> {
        if limits.max_primary_patterns_per_entry == 0
            || limits.max_secondary_patterns_per_entry == 0
            || limits.max_pattern_bytes == 0
            || limits.max_regex_program_bytes == 0
            || limits.max_groups_per_entry == 0
            || limits.max_group_key_bytes == 0
        {
            return Err(ActivationRuleValidationError::InvalidLimit);
        }
        if self.mode.constant && self.mode.exact_target_only {
            return Err(ActivationRuleValidationError::ConflictingModes);
        }
        if self.mode.enabled && self.match_rule.keys.is_empty() && !self.mode.constant && !self.mode.exact_target_only {
            return Err(ActivationRuleValidationError::MissingActivationSource);
        }
        if self.selection.probability > 100 {
            return Err(ActivationRuleValidationError::InvalidProbability);
        }
        if self.match_rule.keys.len() > limits.max_primary_patterns_per_entry {
            return Err(ActivationRuleValidationError::TooManyPrimaryPatterns);
        }
        if self.match_rule.secondary_keys.len() > limits.max_secondary_patterns_per_entry {
            return Err(ActivationRuleValidationError::TooManySecondaryPatterns);
        }
        if self.selection.groups.len() > limits.max_groups_per_entry {
            return Err(ActivationRuleValidationError::TooManyGroups);
        }
        for pattern in self.match_rule.keys.iter().chain(self.match_rule.secondary_keys.iter()) {
            validate_pattern(pattern, limits, compile_regex)?;
        }
        for group in &self.selection.groups {
            if group.as_str().len() > limits.max_group_key_bytes {
                return Err(ActivationRuleValidationError::GroupKeyTooLong);
            }
            Self::validate_group(group)?;
        }
        if self.recursion.delay_until_recursion == Some(0) {
            return Err(ActivationRuleValidationError::InvalidPattern);
        }
        Ok(())
    }

    fn validate_group(group: &ActivationGroupKey) -> Result<(), ActivationRuleValidationError> {
        ActivationGroupKey::try_new(group.as_str().to_owned()).map(|_| ())
    }
}

fn validate_pattern(
    pattern: &ActivationPattern,
    limits: ActivationRuleLimits,
    compile_regex: bool,
) -> Result<(), ActivationRuleValidationError> {
    let ActivationPattern::Literal(raw) = pattern;
    if raw.len() > limits.max_pattern_bytes {
        return Err(ActivationRuleValidationError::PatternTooLong);
    }
    let value = raw.trim();
    if value.is_empty() {
        return Err(ActivationRuleValidationError::EmptyPattern);
    }
    if value.contains("{{") || value.contains("}}") {
        validate_macro_pattern(value)?;
    }
    if let Some((expression, flags)) = regex_parts(value) {
        if value.contains("{{") {
            return Err(ActivationRuleValidationError::InvalidMacro);
        }
        if compile_regex {
            compile_activation_regex(expression, flags, limits.max_regex_program_bytes)?;
        }
    }
    Ok(())
}

pub fn compile_activation_regex(
    expression: &str,
    flags: &str,
    max_program_bytes: usize,
) -> Result<regex::Regex, ActivationRuleValidationError> {
    if max_program_bytes == 0 {
        return Err(ActivationRuleValidationError::InvalidLimit);
    }
    if flags.chars().any(|flag| !matches!(flag, 'i' | 'm' | 's' | 'u'))
        || flags.chars().count() != flags.chars().collect::<std::collections::BTreeSet<_>>().len()
    {
        return Err(ActivationRuleValidationError::InvalidRegex);
    }
    let mut builder = regex::RegexBuilder::new(expression);
    builder
        .case_insensitive(flags.contains('i'))
        .multi_line(flags.contains('m'))
        .dot_matches_new_line(flags.contains('s'))
        .unicode(true);
    builder
        .clone()
        .build()
        .map_err(|_| ActivationRuleValidationError::InvalidRegex)?;
    builder
        .size_limit(max_program_bytes)
        .build()
        .map_err(|_| ActivationRuleValidationError::RegexProgramTooLarge)
}

fn validate_macro_pattern(value: &str) -> Result<(), ActivationRuleValidationError> {
    let mut remaining = value;
    while let Some(start) = remaining.find("{{") {
        let end = remaining[start..]
            .find("}}")
            .ok_or(ActivationRuleValidationError::InvalidMacro)?;
        let name = &remaining[start + 2..start + end];
        if !matches!(name, "player_name" | "player_role_label") {
            return Err(ActivationRuleValidationError::InvalidMacro);
        }
        remaining = &remaining[start + end + 2..];
    }
    if remaining.contains("}}") {
        return Err(ActivationRuleValidationError::InvalidMacro);
    }
    Ok(())
}

fn regex_parts(value: &str) -> Option<(&str, &str)> {
    let rest = value.strip_prefix('/')?;
    let end = rest.rfind('/')?;
    Some((&rest[..end], &rest[end + 1..]))
}

pub fn normalize_activation_literal(value: &str, case_sensitive: bool) -> String {
    let mut normalized = String::new();
    let mut pending_space = false;
    for character in value.chars() {
        if character.is_whitespace() {
            if !normalized.is_empty() {
                pending_space = true;
            }
        } else {
            if pending_space {
                normalized.push(' ');
                pending_space = false;
            }
            normalized.extend(if case_sensitive {
                character.to_string().chars().collect::<Vec<_>>()
            } else {
                character.to_lowercase().collect::<Vec<_>>()
            });
        }
    }
    normalized
}

fn default_true() -> bool {
    true
}

fn default_probability() -> u8 {
    100
}

fn default_group_weight() -> u32 {
    100
}

#[cfg(test)]
#[path = "tests/rule_tests.rs"]
mod tests;
