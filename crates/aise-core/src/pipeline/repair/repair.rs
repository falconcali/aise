use crate::core::{StoryContext, TurnControl, TurnEventSink};
use crate::pipeline::common::{Pipeline, PipelineError, PipelineStage};
use crate::pipeline::validate::ValidateScoreResult;
use crate::trace::Observation;
use serde::Serialize;

#[derive(Serialize)]
pub struct RepairInput {
    pub original_proposal: String,
    pub current_proposal: String,
    pub scores: Vec<ValidateScoreResult>,
    pub proposal_version: u32,
}

#[derive(Serialize)]
pub struct RepairOutput {
    pub original_proposal: String,
    pub current_proposal: String,
    pub proposal_version: u32,
}

pub struct RepairPipeline;

impl Pipeline for RepairPipeline {
    type Input = RepairInput;
    type Output = RepairOutput;

    fn stage(&self) -> PipelineStage {
        PipelineStage::Repair
    }

    async fn execute(
        &self,
        story_ctx: &StoryContext,
        input: Self::Input,
        control: &TurnControl,
        sink: &dyn TurnEventSink,
        observation: &Observation,
    ) -> Result<Self::Output, PipelineError> {
        Ok(RepairOutput {
            original_proposal: input.original_proposal,
            current_proposal: input.current_proposal,
            proposal_version: input.proposal_version,
        })
    }
}
