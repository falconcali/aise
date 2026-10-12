use crate::core::activation::ActivationRule;
use crate::core::knowledge::{EntryOrigin, Recallable, RetrievalHint};
use crate::core::{BoundedText, RumorId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RumorBody {
    pub content: BoundedText,
    #[serde(default)]
    pub activation: ActivationRule,
    pub retrieval: RetrievalHint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RumorEntry {
    pub id: RumorId,
    pub origin: EntryOrigin,
    pub body: RumorBody,
}

impl Recallable for RumorBody {
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
