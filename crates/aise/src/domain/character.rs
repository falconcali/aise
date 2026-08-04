use super::ids::CharacterId;
use serde::{Deserialize, Serialize};

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
