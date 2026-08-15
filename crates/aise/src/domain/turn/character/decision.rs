use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::CharacterId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterDecision {
    pub character_id: CharacterId,
    pub decision: BoundedText,
    pub suggested_utterance: Option<BoundedText>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CharacterDecisionOutput {
    pub decision: BoundedText,
    pub suggested_utterance: Option<BoundedText>,
}
