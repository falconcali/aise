use crate::domain::asset::validation::{AssetValidationCode, AssetValidationError};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::Arc;

macro_rules! key_type {
    ($name:ident) => {
        #[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(Arc<str>);

        impl $name {
            pub fn try_new(value: impl Into<String>) -> Result<Self, AssetValidationError> {
                let value = value.into();
                if !is_valid_key(&value) {
                    return Err(AssetValidationError::Invalid {
                        code: AssetValidationCode::InvalidKey,
                        path: value,
                    });
                }
                Ok(Self(Arc::from(value)))
            }

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
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
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

macro_rules! semantic_key_type {
    ($name:ident) => {
        #[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(Arc<str>);

        impl $name {
            pub fn try_new(value: impl Into<String>) -> Result<Self, AssetValidationError> {
                let value = value.into();
                if !is_valid_semantic_key(&value) {
                    return Err(AssetValidationError::Invalid {
                        code: AssetValidationCode::InvalidKey,
                        path: value,
                    });
                }
                Ok(Self(Arc::from(value)))
            }

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
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
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

fn is_valid_key(value: &str) -> bool {
    if value.is_empty() {
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

fn is_valid_semantic_key(value: &str) -> bool {
    !value.trim().is_empty() && !value.chars().any(char::is_control)
}

key_type!(WorldBookKey);
key_type!(StoryPackKey);
key_type!(SceneKey);
key_type!(LocationKey);
key_type!(EntityKey);
semantic_key_type!(TopicKey);
key_type!(FactKey);
key_type!(RumorKey);
key_type!(MemoryKey);
key_type!(NarrativeNodeKey);
key_type!(NarrativeEdgeKey);
key_type!(NarrativeConditionKey);
key_type!(CanonicalEventKey);
key_type!(AssetId);
key_type!(PackId);
key_type!(PlayerId);
key_type!(AttributeKey);
semantic_key_type!(RelationshipKind);
semantic_key_type!(MemoryKind);
key_type!(ConstraintKey);
key_type!(InstanceSettingKey);

#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SemanticVersion(Arc<str>);

impl SemanticVersion {
    pub fn try_new(value: impl Into<String>) -> Result<Self, AssetValidationError> {
        let value = value.into();
        if !is_valid_semver(&value) {
            return Err(AssetValidationError::Invalid {
                code: AssetValidationCode::InvalidVersion,
                path: value,
            });
        }
        Ok(Self(Arc::from(value)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn is_valid_semver(value: &str) -> bool {
    let core = match value.split(['-', '+']).next() {
        Some(core) => core,
        None => return false,
    };
    let mut parts = core.split('.');
    let major = match parts.next().and_then(|part| part.parse::<u64>().ok()) {
        Some(major) => major,
        None => return false,
    };
    let minor = match parts.next().and_then(|part| part.parse::<u64>().ok()) {
        Some(minor) => minor,
        None => return false,
    };
    let patch = match parts.next().and_then(|part| part.parse::<u64>().ok()) {
        Some(patch) => patch,
        None => return false,
    };
    if parts.next().is_some() {
        return false;
    }
    if major == 0 && minor == 0 && patch == 0 {
        return false;
    }
    if let Some(rest) = value.split('-').nth(1) {
        let pre = match rest.split('+').next() {
            Some(pre) => pre,
            None => return false,
        };
        if pre.is_empty() {
            return false;
        }
    }
    if let Some(build) = value.split('+').nth(1) {
        if build.is_empty() {
            return false;
        }
    }
    true
}

impl fmt::Display for SemanticVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for SemanticVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("SemanticVersion").field(&self.0).finish()
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Sha256Digest([u8; 32]);

impl Sha256Digest {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn try_new(value: &str) -> Result<Self, AssetValidationError> {
        let hex = value.strip_prefix("sha256:").unwrap_or(value);
        if hex.len() != 64 {
            return Err(AssetValidationError::Invalid {
                code: AssetValidationCode::InvalidKey,
                path: value.to_string(),
            });
        }
        let mut out = [0u8; 32];
        for (index, pair) in hex.as_bytes().chunks_exact(2).enumerate() {
            let high = hex_val(pair[0]).ok_or_else(|| AssetValidationError::Invalid {
                code: AssetValidationCode::InvalidKey,
                path: value.to_string(),
            })?;
            let low = hex_val(pair[1]).ok_or_else(|| AssetValidationError::Invalid {
                code: AssetValidationCode::InvalidKey,
                path: value.to_string(),
            })?;
            out[index] = (high << 4) | low;
        }
        Ok(Self(out))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

fn hex_val(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

impl Serialize for Sha256Digest {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut hex = String::with_capacity(65);
        hex.push_str("sha256:");
        for byte in &self.0 {
            hex.push_str(&format!("{byte:02x}"));
        }
        serializer.serialize_str(&hex)
    }
}

impl<'de> Deserialize<'de> for Sha256Digest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::try_new(&value).map_err(serde::de::Error::custom)
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Sha256Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Sha256Digest").field(&self.0).finish()
    }
}
