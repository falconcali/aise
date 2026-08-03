use serde::{Deserialize, Serialize};

use super::ids::CharacterId;

/// Persisted state of one character. `internal_state` is a character's
/// viewpoint and MUST NOT be committed as world fact (R-AISE-07).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterState {
    pub id: CharacterId,
    pub name: String,
    pub bio: String,
    pub internal_state: InternalState,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InternalState {
    pub goals: Vec<String>,
    pub health: i32,
    pub relationships: Vec<Relation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    pub other: CharacterId,
    pub affinity: i32,
}

/// A requested mutation to one character, produced by a story draft.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterPatch {
    pub id: CharacterId,
    pub set_health: Option<i32>,
    pub set_goals: Option<Vec<String>>,
    pub adjust_affinity: Vec<(CharacterId, i32)>,
}
