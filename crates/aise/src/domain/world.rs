use super::character::CharacterState;
use super::ids::StoryId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorldState {
    pub id: StoryId,
    pub name: String,

    pub facts: Vec<WorldFact>,
    pub characters: Vec<CharacterState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldFact {
    pub text: String,
    pub source: FactSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FactSource {
    Seed,
    CommittedTurn,
    UserEdit,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorldPatch {
    pub add_facts: Vec<WorldFact>,
    pub remove_fact_indices: Vec<usize>,
}
