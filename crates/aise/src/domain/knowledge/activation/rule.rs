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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ActivationRuleVersion(pub Sha256Digest);

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
    #[error("activation group count exceeds the configured limit")]
    TooManyGroups,
    #[error("activation pattern is empty")]
    EmptyPattern,
    #[error("activation pattern is invalid")]
    InvalidPattern,
    #[error("activation regex is invalid")]
    InvalidRegex,
    #[error("activation macro is invalid")]
    InvalidMacro,
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

    pub fn validate(&self, max_groups: usize) -> Result<(), ActivationRuleValidationError> {
        if self.mode.constant && self.mode.exact_target_only {
            return Err(ActivationRuleValidationError::ConflictingModes);
        }
        if self.mode.enabled && self.match_rule.keys.is_empty() && !self.mode.constant && !self.mode.exact_target_only {
            return Err(ActivationRuleValidationError::MissingActivationSource);
        }
        if self.selection.probability > 100 {
            return Err(ActivationRuleValidationError::InvalidProbability);
        }
        if self.selection.groups.len() > max_groups {
            return Err(ActivationRuleValidationError::TooManyGroups);
        }
        for pattern in self.match_rule.keys.iter().chain(self.match_rule.secondary_keys.iter()) {
            validate_pattern(pattern)?;
        }
        for group in &self.selection.groups {
            Self::validate_group(group)?;
        }
        if self.recursion.delay_until_recursion == Some(0) {
            return Err(ActivationRuleValidationError::InvalidPattern);
        }
        Ok(())
    }

    pub fn rule_version(&self) -> Result<ActivationRuleVersion, serde_json::Error> {
        let bytes = serde_json::to_vec(self)?;
        let digest = Sha256::digest(bytes);
        Ok(ActivationRuleVersion(Sha256Digest::from_bytes(digest.into())))
    }

    fn validate_group(group: &ActivationGroupKey) -> Result<(), ActivationRuleValidationError> {
        Self::validate_group_text(group.as_str())
    }

    fn validate_group_text(value: &str) -> Result<(), ActivationRuleValidationError> {
        ActivationGroupKey::try_new(value.to_owned()).map(|_| ())
    }
}

fn validate_pattern(pattern: &ActivationPattern) -> Result<(), ActivationRuleValidationError> {
    let ActivationPattern::Literal(value) = pattern;
    let value = value.trim();
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
        if flags.chars().any(|flag| !matches!(flag, 'i' | 'm' | 's' | 'u'))
            || flags.chars().count() != flags.chars().collect::<std::collections::BTreeSet<_>>().len()
        {
            return Err(ActivationRuleValidationError::InvalidRegex);
        }
        regex::RegexBuilder::new(expression)
            .case_insensitive(flags.contains('i'))
            .multi_line(flags.contains('m'))
            .dot_matches_new_line(flags.contains('s'))
            .unicode(true)
            .build()
            .map_err(|_| ActivationRuleValidationError::InvalidRegex)?;
    }
    Ok(())
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
