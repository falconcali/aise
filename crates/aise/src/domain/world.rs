use super::ids::{FactId, StoryId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldState {
    pub id: StoryId,
    pub name: String,

    pub facts: Vec<WorldFact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldFact {
    pub id: FactId,
    pub text: String,
    pub source: FactSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FactSource {
    Seed,
    CommittedTurn,
    UserEdit,
}
