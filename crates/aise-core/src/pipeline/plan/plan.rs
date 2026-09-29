use crate::core::story::StoryContext;
use crate::core::PlayerContribution;
use crate::pipeline::common::Pipeline;

pub struct PlanInput {
    pub story_ctx: StoryContext,
    pub player_contributions: Vec<PlayerContribution>
}

pub struct PlanOutput {
    pub plan: String
}

pub struct PlanPipeline;

impl Pipeline for PlanPipeline {
    type Input = PlanInput;
    type Output = PlanOutput;

    fn stage(&self) -> &'static str {
        "pipeline_plan"
    }

    fn execute(
        &self,
        input: Self::Input,
        control: &TurnControl,
        sink: &dyn TurnEventSink,
        observation: &Observation
    ) -> Result<Self::Output, PipelineError> {
        let plan = format!(
            "Story ID: {}\nPlayer Contributions:\n{}",
            input.story_ctx.story_id.0,
            input.player_contributions.iter().map(|c| format!("{:?}", c)).collect::<Vec<_>>().join("\n")
        );

        Ok(PlanOutput { plan })
    }
}