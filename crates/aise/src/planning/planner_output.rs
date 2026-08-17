use crate::domain::asset::validation::BoundedText;
use crate::domain::turn::{CharacterThinkRequest, KnowledgeDelivery};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerOutput {
    pub story_goal: BoundedText,
    pub context_gaps: Vec<PlannerContextGap>,
    pub character_think_requests: Vec<CharacterThinkRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerContextGap {
    pub delivery: KnowledgeDelivery,
    pub target_id: Option<String>,
    pub query_text: Option<BoundedText>,
    pub reason: BoundedText,
}
