use serde::{Deserialize, Serialize};

use crate::domain::ids::CharacterId;

/// A character's viewpoint: perception, emotion, goal, and leaning. NOT world
/// facts and MUST NOT be committed as world state (R-AISE-07).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterThought {
    pub character_id: CharacterId,
    pub perception: String,
    pub emotion: String,
    pub goal: String,
    pub possible_action: String,
}
