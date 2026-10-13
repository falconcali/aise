use crate::core::{BoundedText, CoreError};
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::num::NonZeroU16;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ActivationRule {
    #[serde(rename = "match")]
    pub match_rule: ActivationMatchRule,
    pub mode: ActivationMode,
    pub recursion: ActivationRecursionRule,
    pub selection: ActivationSelectionRule,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ActivationMatchRule {
    pub keys: Vec<ActivationKey>,
    pub secondary_keys: Vec<ActivationKey>,
    pub secondary_logic: SecondaryLogic,
    pub case_sensitive: bool,
    pub match_whole_words: bool,
    pub scan_depth: Option<u16>,
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

impl SecondaryLogic {
    pub(super) fn holds(self, matched: usize, total: usize) -> bool {
        if total == 0 {
            return true;
        }
        match self {
            Self::AndAny => matched > 0,
            Self::AndAll => matched == total,
            Self::NotAny => matched == 0,
            Self::NotAll => matched < total,
        }
    }

    pub(super) fn scores_matches(self) -> bool {
        matches!(self, Self::AndAny | Self::AndAll)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ActivationMode {
    pub enabled: bool,
    pub constant: bool,
}

impl Default for ActivationMode {
    fn default() -> Self {
        Self {
            enabled: true,
            constant: false,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ActivationRecursionRule {
    pub exclude_recursion: bool,
    pub prevent_recursion: bool,
    pub delay_until_recursion: Option<NonZeroU16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ActivationSelectionRule {
    pub order: i32,
    pub probability: ActivationProbability,
    pub groups: Vec<ActivationGroupKey>,
    pub group_override: bool,
    pub group_weight: u32,
    pub use_group_scoring: bool,
}

impl Default for ActivationSelectionRule {
    fn default() -> Self {
        Self {
            order: 0,
            probability: ActivationProbability::ALWAYS,
            groups: Vec::new(),
            group_override: false,
            group_weight: 100,
            use_group_scoring: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ActivationKey(BoundedText);

impl ActivationKey {
    pub const MAX_BYTES: usize = 128;
    pub const LIMIT_NAME: &str = "activation_key";

    pub fn try_new(value: impl Into<String>) -> Result<Self, CoreError> {
        let value = value.into();
        let trim_value = value.trim();
        if trim_value.is_empty() {
            return Err(CoreError::ActivationKeyEmpty);
        }
        let bounded_text =
            BoundedText::try_new(trim_value, Self::LIMIT_NAME, Self::MAX_BYTES).map_err(|e| match e {
                CoreError::BoundedTextLimitExceeded {
                    limit: Self::LIMIT_NAME,
                    actual,
                    maximum,
                } => CoreError::ActivationKeyTooLong { actual, maximum },
                _ => e,
            })?;
        Ok(Self(bounded_text))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl TryFrom<String> for ActivationKey {
    type Error = CoreError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl From<ActivationKey> for String {
    fn from(key: ActivationKey) -> Self {
        key.0.into_string()
    }
}

impl Display for ActivationKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ActivationGroupKey(String);

impl ActivationGroupKey {
    pub const MAX_BYTES: usize = 128;

    pub fn try_new(value: impl Into<String>) -> Result<Self, CoreError> {
        let value = value.into();
        if value.is_empty() || value.len() > Self::MAX_BYTES || !is_group_key(&value) {
            return Err(CoreError::ActivationGroupKeyInvalid { value });
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ActivationGroupKey {
    type Error = CoreError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl From<ActivationGroupKey> for String {
    fn from(key: ActivationGroupKey) -> Self {
        key.0
    }
}

impl Display for ActivationGroupKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct ActivationProbability(u8);

impl ActivationProbability {
    pub const ALWAYS: Self = Self(100);

    pub fn try_new(percent: u8) -> Result<Self, CoreError> {
        if percent > 100 {
            return Err(CoreError::ActivationProbabilityInvalid { value: percent });
        }
        Ok(Self(percent))
    }

    pub fn percent(self) -> u8 {
        self.0
    }
}

impl Default for ActivationProbability {
    fn default() -> Self {
        Self::ALWAYS
    }
}

impl TryFrom<u8> for ActivationProbability {
    type Error = CoreError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl From<ActivationProbability> for u8 {
    fn from(probability: ActivationProbability) -> Self {
        probability.0
    }
}

fn is_group_key(value: &str) -> bool {
    let mut previous_separator = true;
    for character in value.chars() {
        if character.is_ascii_lowercase() || character.is_ascii_digit() {
            previous_separator = false;
        } else if matches!(character, '.' | '_' | '-') && !previous_separator {
            previous_separator = true;
        } else {
            return false;
        }
    }
    !previous_separator
}

#[cfg(test)]
#[path = "tests/rule_tests.rs"]
mod tests;
