use serde::{Deserialize, Serialize};

use super::character::CharacterState;
use super::ids::StoryId;

/// The persisted truth of a story's setting.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorldState {
    pub id: StoryId,
    pub name: String,
    /// Free-form world knowledge; may be augmented by retrieval.
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

/// A requested mutation to world state, produced by a story draft.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorldPatch {
    pub add_facts: Vec<WorldFact>,
    pub remove_fact_indices: Vec<usize>,
}
