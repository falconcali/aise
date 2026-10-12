use crate::core::{CharacterCardId, SemanticVersion, Sha256Digest};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterCardRef {
    pub card_id: CharacterCardId,
    pub version: SemanticVersion,
    pub digest: Sha256Digest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "card", rename_all = "snake_case")]
pub enum RoleActor {
    Default,
    Card(CharacterCardRef),
}
