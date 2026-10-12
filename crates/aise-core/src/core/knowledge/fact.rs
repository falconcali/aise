use crate::core::activation::ActivationRule;
use crate::core::knowledge::{EntryOrigin, Recallable, RetrievalHint};
use crate::core::{BoundedText, FactId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactBody {
    pub content: BoundedText,
    #[serde(default)]
    pub activation: ActivationRule,
    pub retrieval: RetrievalHint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactEntry {
    pub id: FactId,
    pub origin: EntryOrigin,
    pub body: FactBody,
}

impl Recallable for FactBody {
    fn content(&self) -> &BoundedText {
        &self.content
    }

    fn activation(&self) -> &ActivationRule {
        &self.activation
    }

    fn retrieval(&self) -> &RetrievalHint {
        &self.retrieval
    }
}
