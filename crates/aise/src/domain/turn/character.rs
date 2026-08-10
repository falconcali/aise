use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::CharacterId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterThought {
    pub character_id: CharacterId,
    pub perception: BoundedText,
    pub emotion: BoundedText,
    pub goal: BoundedText,
    pub possible_action: BoundedText,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CharacterThoughtOutput {
    pub perception: BoundedText,
    pub emotion: BoundedText,
    pub goal: BoundedText,
    pub possible_action: BoundedText,
}
