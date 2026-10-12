use serde::{Deserialize, Serialize};
use crate::core::{RetreivalHint, BoundedText, CharacterProfile, MemoryBody};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Role {
    pub role_id: RoleId,
    pub profile: RoleProfile,
    pub memories: BTreeMap<MemoryId, MemoryEntry>,
}
