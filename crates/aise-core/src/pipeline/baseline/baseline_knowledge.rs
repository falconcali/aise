use crate::core::{FactEntry, PlayerContribution, RumorEntry, StoryContext, StoryPack};
use crate::pipeline::baseline::baseline_trace;
use crate::pipeline::common::PipelineError;
use crate::trace::Observation;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ActivatedKnowledge {
    pub facts: Vec<FactEntry>,
    pub rumors: Vec<RumorEntry>,
}

pub fn activate_knowledge(
    story_pack: &StoryPack,
    story_ctx: &StoryContext,
    player_contribution: &PlayerContribution,
    observation: &Observation,
) -> Result<ActivatedKnowledge, PipelineError> {
    let knowledge_observation = baseline_trace::begin_knowledge_activation(observation, player_contribution);
    let result = build_knowledge(story_pack, story_ctx, player_contribution);
    baseline_trace::finish_knowledge_activation(knowledge_observation, &result);
    result
}

fn build_knowledge(
    _story_pack: &StoryPack,
    _story_ctx: &StoryContext,
    _player_contribution: &PlayerContribution,
) -> Result<ActivatedKnowledge, PipelineError> {
    Ok(ActivatedKnowledge {
        facts: vec![],
        rumors: vec![],
    })
}
