use crate::core::CoreError;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

macro_rules! define_id {
    ($name:ident, $field:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub fn try_new(value: impl Into<String>) -> Result<Self, CoreError> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(CoreError::InvalidId { field: $field, value });
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = CoreError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::try_new(value)
            }
        }

        impl From<$name> for String {
            fn from(id: $name) -> Self {
                id.0
            }
        }

        impl Display for $name {
            fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

define_id!(CharacterCardId, "character_card_id");
define_id!(CharacterId, "character_id");
define_id!(IdempotencyKey, "idempotency_key");
define_id!(PackId, "pack_id");
define_id!(PlayerId, "player_id");
define_id!(RoleId, "role_id");
define_id!(SemanticVersion, "version");
define_id!(Sha256Digest, "digest");
define_id!(StoryId, "story_id");
define_id!(MemoryId, "memory_id");
define_id!(FactId, "fact_id");
define_id!(RumorId, "rumor_id");
define_id!(NarrativeNodeId, "narrative_node_id");
define_id!(NarrativeConditionId, "narrative_condition_id");
