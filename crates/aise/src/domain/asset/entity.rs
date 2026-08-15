use crate::domain::asset::ids::{CanonicalEventKey, EntityKey, LocationKey, NarrativeNodeKey, SceneKey};
use crate::domain::ids::{CharacterId, RoleId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", content = "key", rename_all = "snake_case", deny_unknown_fields)]
pub enum KnowledgeEntity {
    World(EntityKey),
    Role(RoleId),
    Character(CharacterId),
    Location(LocationKey),
    Scene(SceneKey),
    NarrativeNode(NarrativeNodeKey),
    Event(CanonicalEventKey),
}
