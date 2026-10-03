use crate::core::{PlayerContribution, StoryContext, TurnControl, TurnEventSink};
use crate::pipeline::common::{Pipeline, PipelineError, PipelineStage};
use crate::trace::Observation;

pub struct BaselineInput {
    pub story_ctx: StoryContext,
    pub player_input: String,
}

pub struct BaselineOutput {
    pub player_contribution: PlayerContribution,
}

pub struct BaselinePipeline;

impl Pipeline for BaselinePipeline {
    type Input = BaselineInput;
    type Output = BaselineOutput;

    fn stage(&self) -> PipelineStage {
        PipelineStage::Baseline
    }

    async fn execute(
        &self,
        input: Self::Input,
        control: &TurnControl,
        sink: &dyn TurnEventSink,
        observation: &Observation,
    ) -> Result<Self::Output, PipelineError> {
        let player_contribution = PlayerContribution {
            raw: input.player_input.clone(),
            processed: format!("{} (Processed by Baseline)", input.player_input.clone()),
        };

        Ok(BaselineOutput { player_contribution })
    }
}
