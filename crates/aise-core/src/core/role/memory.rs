use crate::core::activation::ActivationRule;
use crate::core::knowledge::{EntryOrigin, Recallable, RetrievalHint};
use crate::core::{BoundedText, MemoryId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryBody {
    pub content: BoundedText,
    #[serde(default)]
    pub activation: ActivationRule,
    pub retrieval: RetrievalHint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: MemoryId,
    pub origin: EntryOrigin,
    pub body: MemoryBody,
}

impl Recallable for MemoryBody {
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
