use crate::domain::asset::validation::BoundedText;
use crate::domain::turn::{CharacterThinkRequest, RetrievalAudience, RetrievalTargetId};
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
    pub audience: RetrievalAudience,
    pub target_id: Option<RetrievalTargetId>,
    pub query_text: Option<BoundedText>,
    pub reason: BoundedText,
}
