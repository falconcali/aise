use crate::pipeline::common::Pipeline;
use crate::core::StoryContext;
use crate::core::PlayerContribution;

pub struct BaselineInput{
    pub story_ctx: StoryContext,
    pub player_input: String
}

pub struct BaselineOutput {
    pub player_contributions: Vec<PlayerContribution>
}

pub struct BaselinePipeline; 

impl Pipeline for BaselinePipeline {
    type Input = BaselineInput;
    type Output = BaselineOutput;

    fn stage(&self) -> &'static str {
        "pipeline_baseline"
    }

    fn execute(
        &self,
        input: Self::Input,
        control: &TurnControl,
        sink: &dyn TurnEventSink,
        observation: &Observation
    ) -> Result<Self::Output, PipelineError> {
        let player_contributions = vec![
            PlayerContribution::Thoughts(input.player_input.clone()),
            PlayerContribution::Actions(input.player_input.clone()),
            PlayerContribution::Speech(input.player_input.clone())
        ];

        Ok(BaselineOutput {
            player_contributions
        })
    }
}