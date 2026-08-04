use crate::prompt::model::{AssetRef, PromptLineageNode, SlotId};

#[derive(Debug, Clone)]
pub struct PromptMetadata {
    pub slot: SlotId,
    pub pack: String,
    pub root: PromptLineageNode,
    pub rendered_assets: Vec<AssetRef>,
    pub applied_policies: Vec<String>,
    pub selection_reason: String,
    pub render_duration_ms: u64,
    pub input_validated: bool,
    pub output_contract_validated: bool,
}
