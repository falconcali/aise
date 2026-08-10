use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::ids::TopicKey;
use crate::domain::asset::validation::BoundedText;
use crate::domain::knowledge::KnowledgeKind;
use crate::domain::turn::{CharacterThinkRequest, RetrievalAudience, WriterStoryGoal};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerOutput {
    pub story_goal: WriterStoryGoal,
    #[serde(default)]
    pub context_gaps: Vec<PlannerContextGap>,
    #[serde(default)]
    pub character_think_requests: Vec<CharacterThinkRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerContextGap {
    pub audience: RetrievalAudience,
    pub knowledge_kinds: Vec<KnowledgeKind>,
    #[serde(default)]
    pub entities: Vec<KnowledgeEntity>,
    #[serde(default)]
    pub topics: Vec<TopicKey>,
    pub query_text: Option<BoundedText>,
    pub reason: BoundedText,
}
