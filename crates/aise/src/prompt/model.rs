use serde::{Deserialize, Serialize};
use std::{borrow::Borrow, fmt, ops::Deref};

macro_rules! string_newtype {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl Borrow<str> for $name {
            fn borrow(&self) -> &str {
                self.as_str()
            }
        }

        impl Deref for $name {
            type Target = str;

            fn deref(&self) -> &Self::Target {
                self.as_str()
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_string())
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl PartialEq<&str> for $name {
            fn eq(&self, other: &&str) -> bool {
                self.as_str() == *other
            }
        }

        impl PartialEq<$name> for &str {
            fn eq(&self, other: &$name) -> bool {
                *self == other.as_str()
            }
        }
    };
}

string_newtype!(SlotId);
string_newtype!(AssetRef);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptKind {
    Text,
    Messages,
    Fragment,
    FewShot,
}

impl std::fmt::Display for PromptKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PromptKind::Text => write!(f, "text"),
            PromptKind::Messages => write!(f, "messages"),
            PromptKind::Fragment => write!(f, "fragment"),
            PromptKind::FewShot => write!(f, "few_shot"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetStatus {
    Active,
    Deprecated,
    Archived,
    Candidate,
}

impl std::fmt::Display for AssetStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AssetStatus::Active => write!(f, "active"),
            AssetStatus::Deprecated => write!(f, "deprecated"),
            AssetStatus::Archived => write!(f, "archived"),
            AssetStatus::Candidate => write!(f, "candidate"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptMessage {
    pub role: PromptRole,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderedPrompt {
    Text(String),
    Messages(Vec<PromptMessage>),
}

impl RenderedPrompt {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            RenderedPrompt::Text(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_messages(&self) -> Option<&[PromptMessage]> {
        match self {
            RenderedPrompt::Messages(messages) => Some(messages),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptLineageNode {
    pub slot: SlotId,
    pub asset_id: AssetRef,
    pub hash: Option<String>,
}

#[cfg(test)]
#[path = "tests/model_tests.rs"]
mod tests;
