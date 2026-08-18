use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::RoleId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterDecision {
    pub role_id: RoleId,
    pub decision: BoundedText,
    pub suggested_utterance: Option<BoundedText>,
}
