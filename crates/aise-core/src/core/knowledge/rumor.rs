use crate::core::activation::ActivationRule;
use crate::core::knowledge::RetrievalHint;
use crate::core::{BoundedText, RumorId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RumorBody {
    pub content: BoundedText,
    pub activation: ActivationRule,
    pub retrieval: RetrievalHint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RumorEntry {
    pub rumor_id: RumorId,
    pub rumor_body: RumorBody,
    pub created_at_ms: u64,
}
