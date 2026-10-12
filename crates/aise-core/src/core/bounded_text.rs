use crate::core::CoreError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BoundedText(String);

impl BoundedText {
    pub fn try_new(value: impl Into<String>, field: &'static str, maximum_bytes: usize) -> Result<Self, CoreError> {
        let value = value.into();
        let actual = value.len();
        if actual > maximum_bytes {
            return Err(CoreError::BoundedTextLimitExceeded {
                limit: field,
                actual: actual as u64,
                maximum: maximum_bytes as u64,
            });
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl Serialize for BoundedText {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for BoundedText {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::try_new(value, "bounded_text", usize::MAX).map_err(serde::de::Error::custom)
    }
}

impl std::fmt::Display for BoundedText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for BoundedText {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::ops::Deref for BoundedText {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
