use crate::core::{PlayerContribution, StoryContext, TurnControl, TurnEventSink};
use crate::pipeline::common::{Pipeline, PipelineError, PipelineStage};
use crate::trace::Observation;
use serde::Serialize;

#[derive(Serialize)]
pub struct PlanInput {
    pub player_contribution: PlayerContribution,
}

#[derive(Serialize)]
pub struct PlanOutput {
    pub plan: String,
}

pub struct PlanPipeline;

impl Pipeline for PlanPipeline {
    type Input = PlanInput;
    type Output = PlanOutput;

    fn stage(&self) -> PipelineStage {
        PipelineStage::Plan
    }

    async fn execute(
        &self,
        input: Self::Input,
        story_ctx: &StoryContext,
        control: &TurnControl,
        sink: &dyn TurnEventSink,
        observation: &Observation,
    ) -> Result<Self::Output, PipelineError> {
        let plan = format!(
            "Story ID: {}\nPlayer Contribution:\n{}",
            story_ctx.story_id, input.player_contribution.processed
        );

        Ok(PlanOutput { plan })
    }
}
