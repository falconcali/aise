use crate::core::BoundedText;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterProfile {
    pub appearance: Option<BoundedText>,
    pub personality: Option<BoundedText>,
    pub speaking_style: Option<BoundedText>,
    #[serde(default)]
    pub dialogue_examples: Vec<DialogueExample>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DialogueExample {
    pub situation: BoundedText,
    pub response: BoundedText,
}
