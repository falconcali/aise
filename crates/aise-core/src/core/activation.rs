use super::{KnowledgeSourceId, StoryId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivationPreviewRequest {
    pub story_id: StoryId,
    pub player_contribution: String,
    pub generation_trigger: GenerationTrigger,
    pub external_targets: Vec<ActivationTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivationTarget {
    pub source_id: KnowledgeSourceId,
    pub delivery: KnowledgeDelivery,
    pub mandatory: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivationPreviewResult {
    pub entries: Vec<ActivationPreviewEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivationPreviewEntry {
    pub source_id: KnowledgeSourceId,
    pub delivery: KnowledgeDelivery,
    pub content: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GenerationTrigger {
    Normal,
    Repair,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeDelivery {
    Context,
    Character,
    Story,
}
