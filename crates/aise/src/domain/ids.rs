use crate::domain::error::{DomainInputError, KnowledgeIdError};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::num::NonZeroU64;
use std::sync::Arc;
use uuid::Uuid;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(Arc<str>);

        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(Arc::from(value))
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(Arc::from(value))
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                Ok(Self(Arc::from(String::deserialize(deserializer)?)))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_tuple(stringify!($name)).field(&self.0).finish()
            }
        }
    };
}

id_type!(EventId);

pub(crate) fn format_knowledge_sequence(sequence: NonZeroU64) -> String {
    let value = sequence.get();
    if value < 10000 {
        format!("{value:04}")
    } else {
        value.to_string()
    }
}

pub(crate) fn parse_knowledge_sequence(text: &str) -> Option<NonZeroU64> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let value: u64 = text.parse().ok()?;
    let sequence = NonZeroU64::new(value)?;
    (format_knowledge_sequence(sequence) == text).then_some(sequence)
}

fn is_canonical_knowledge_id_shape(value: &str) -> bool {
    [FactId::PREFIX, RumorId::PREFIX, MemoryId::PREFIX]
        .into_iter()
        .any(|prefix| value.strip_prefix(prefix).and_then(parse_knowledge_sequence).is_some())
}

macro_rules! knowledge_id_type {
    ($name:ident, $prefix:literal) => {
        #[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(Arc<str>);

        impl $name {
            pub const PREFIX: &'static str = $prefix;

            pub fn try_new(value: impl Into<String>) -> Result<Self, KnowledgeIdError> {
                let value = value.into();
                let sequence = value
                    .strip_prefix(Self::PREFIX)
                    .and_then(parse_knowledge_sequence)
                    .ok_or_else(|| KnowledgeIdError::InvalidGrammar { value: value.clone() })?;
                Self::from_sequence(sequence)
            }

            pub(crate) fn from_sequence(sequence: NonZeroU64) -> Result<Self, KnowledgeIdError> {
                if sequence.get() > i64::MAX as u64 {
                    return Err(KnowledgeIdError::SequenceOverflow);
                }
                Ok(Self(Arc::from(format!(
                    "{}{}",
                    Self::PREFIX,
                    format_knowledge_sequence(sequence)
                ))))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = String::deserialize(deserializer)?;
                Self::try_new(value).map_err(serde::de::Error::custom)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_tuple(stringify!($name)).field(&self.0).finish()
            }
        }
    };
}

knowledge_id_type!(FactId, "fact_");
knowledge_id_type!(RumorId, "rumor_");
knowledge_id_type!(MemoryId, "memory_");

#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CharacterId(Arc<str>);

impl CharacterId {
    pub fn new_uuid() -> Self {
        Self(Arc::from(Uuid::new_v4().to_string()))
    }

    pub fn try_new(value: impl Into<String>) -> Result<Self, DomainInputError> {
        let value = value.into();
        let parsed = Uuid::parse_str(&value).map_err(|_| DomainInputError::InvalidCharacterId)?;
        if parsed.is_nil() {
            return Err(DomainInputError::InvalidCharacterId);
        }
        Ok(Self(Arc::from(parsed.hyphenated().to_string())))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CharacterId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for CharacterId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("CharacterId").field(&self.0).finish()
    }
}

impl Serialize for CharacterId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for CharacterId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::try_new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RoleId(Arc<str>);

impl RoleId {
    pub const MAX_BYTES: usize = 128;

    pub fn try_new(value: impl Into<String>) -> Result<Self, DomainInputError> {
        let value = value.into();
        if !is_valid_role_id(&value) {
            return Err(DomainInputError::InvalidRoleId);
        }
        if is_canonical_knowledge_id_shape(&value) {
            return Err(DomainInputError::RoleIdReservedForKnowledge);
        }
        Ok(Self(Arc::from(value)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn is_valid_role_id(value: &str) -> bool {
    if value.is_empty() || value.len() > RoleId::MAX_BYTES {
        return false;
    }
    let mut seen_segment = false;
    for ch in value.chars() {
        match ch {
            'a'..='z' | '0'..='9' => {
                seen_segment = true;
            }
            '.' | '_' | '-' => {
                if !seen_segment {
                    return false;
                }
                seen_segment = false;
            }
            _ => return false,
        }
    }
    seen_segment
}

impl fmt::Display for RoleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for RoleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("RoleId").field(&self.0).finish()
    }
}

impl Serialize for RoleId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for RoleId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::try_new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StoryId(Arc<str>);

impl StoryId {
    pub fn try_new(value: impl Into<String>) -> Result<Self, DomainInputError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(DomainInputError::EmptyStoryId);
        }
        Ok(Self(Arc::from(value)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for StoryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for StoryId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for StoryId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::try_new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TurnId(Arc<str>);

impl TurnId {
    pub fn try_new(value: impl Into<String>) -> Result<Self, DomainInputError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(DomainInputError::EmptyTurnId);
        }
        Ok(Self(Arc::from(value)))
    }

    pub fn new_uuid() -> Self {
        Self(Arc::from(Uuid::new_v4().to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TurnId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for TurnId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for TurnId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::try_new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConstraintId(Arc<str>);

impl ConstraintId {
    pub fn try_new(value: impl Into<String>) -> Result<Self, DomainInputError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(DomainInputError::EmptyConstraintId);
        }
        Ok(Self(Arc::from(value)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ConstraintId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for ConstraintId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ConstraintId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::try_new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StoryRevision(u64);

impl StoryRevision {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(&self) -> u64 {
        self.0
    }
}

impl fmt::Display for StoryRevision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
#[path = "tests/ids_tests.rs"]
mod tests;
