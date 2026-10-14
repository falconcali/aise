use crate::core::{BoundedText, PlayerContribution, StoryContext, TurnControl};
use crate::pipeline::common::{Pipeline, PipelineError, PipelineStage};
use crate::pipeline::validate::ValidateScoreResult;
use crate::trace::Observation;
use serde::Serialize;

#[derive(Serialize)]
pub struct RepairInput<'a> {
    pub original_proposal: &'a str,
    pub current_proposal: &'a str,
    pub scores: &'a [ValidateScoreResult],
    pub proposal_version: u32,
    pub constraints: &'a [BoundedText],
    pub player_contribution: &'a PlayerContribution,
}

#[derive(Serialize)]
pub struct RepairOutput {
    pub current_proposal: String,
}

pub struct RepairPipeline;

impl<'a> Pipeline<RepairInput<'a>> for RepairPipeline {
    type Output = RepairOutput;

    fn stage(&self) -> PipelineStage {
        PipelineStage::Repair
    }

    async fn execute(
        &self,
        input: RepairInput<'a>,
        _story_ctx: &StoryContext,
        _control: &TurnControl,
        _observation: &Observation,
    ) -> Result<Self::Output, PipelineError> {
        Ok(RepairOutput {
            current_proposal: input.current_proposal.to_owned(),
        })
    }
}
