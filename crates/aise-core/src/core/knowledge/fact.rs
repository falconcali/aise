use crate::core::activation::ActivationRule;
use crate::core::knowledge::RetrievalHint;
use crate::core::{BoundedText, FactId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactBody {
    pub content: BoundedText,
    pub activation: ActivationRule,
    pub retrieval: RetrievalHint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactEntry {
    pub fact_id: FactId,
    pub fact_body: FactBody,
    pub created_at_ms: u64,
}
