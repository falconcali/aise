use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use thiserror::Error;

#[derive(Debug, Error)]
#[error("invalid {field}: {value}")]
pub struct InvalidId {
    field: &'static str,
    value: String,
}

macro_rules! define_id {
    ($name:ident, $field:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub struct $name(String);

        impl $name {
            pub fn try_new(value: impl Into<String>) -> Result<Self, InvalidId> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(InvalidId { field: $field, value });
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Display for $name {
            fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

define_id!(CharacterId, "character_id");
define_id!(IdempotencyKey, "idempotency_key");
define_id!(KnowledgeSourceId, "knowledge_source_id");
define_id!(PackId, "pack_id");
define_id!(PlayerId, "player_id");
define_id!(RoleId, "role_id");
define_id!(SemanticVersion, "version");
define_id!(Sha256Digest, "digest");
define_id!(StoryId, "story_id");
