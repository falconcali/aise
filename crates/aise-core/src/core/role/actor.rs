use crate::core::CharacterCardRef;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "card", rename_all = "snake_case")]
pub enum RoleActor {
    Default,
    Card(CharacterCardRef),
}
