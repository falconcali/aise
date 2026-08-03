use crate::domain::ids::CharacterId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterThought {
    pub character_id: CharacterId,
    pub perception: String,
    pub emotion: String,
    pub goal: String,
    pub possible_action: String,
}
