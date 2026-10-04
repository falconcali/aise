use crate::core::{PlayerContribution, StoryContext, TurnControl, TurnEventSink};
use crate::llm::LlmGateway;
use crate::pipeline::baseline::{baseline_llm, baseline_trace, baseline_prompt};
use crate::pipeline::common::{Pipeline, PipelineError, PipelineStage};
use crate::trace::Observation;
use crate::prompt::Prompt;
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
pub struct BaselineInput {
    pub story_ctx: StoryContext,
    pub player_input: String,
}

#[derive(Serialize)]
pub struct BaselineOutput {
    pub player_contribution: PlayerContribution,
}

pub struct BaselinePipeline {
    gateway: Arc<LlmGateway>,
    prompt: Arc<Prompt>,
}

impl BaselinePipeline {
    pub fn new(gateway: Arc<LlmGateway>, prompt: Arc<Prompt>) -> Self {
        Self { gateway, prompt }
    }
}

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
        let step = baseline_trace::begin_player_contribution(observation, &input.player_input);

        let prompt_result = baseline_prompt::process_player_input(self.prompt.as_ref(), &input.player_input, observation)?;

        match prompt_result {
        let result =
            baseline_llm::process_player_input(self.gateway.as_ref(), &input.player_input, control, &step).await?;

        let player_contribution = PlayerContribution {
            raw: input.player_input.clone(),
            processed: result.content,
        };

        baseline_trace::finish_player_contribution(step, &player_contribution);
        Ok(BaselineOutput { player_contribution })
    }
}
