use crate::core::{PlayerContribution, StoryContext, TurnControl, TurnEventSink};
use crate::llm::LlmGateway;
use crate::pipeline::baseline::{baseline_llm, baseline_prompt, baseline_trace};
use crate::pipeline::common::{Pipeline, PipelineError, PipelineStage};
use crate::prompt::Prompt;
use crate::trace::Observation;
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
pub struct BaselineInput {
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
        story_ctx: &StoryContext,
        input: Self::Input,
        control: &TurnControl,
        sink: &dyn TurnEventSink,
        observation: &Observation,
    ) -> Result<Self::Output, PipelineError> {
        let baseline_observation = baseline_trace::begin_player_contribution(observation, &input.player_input);
        let result = self.process_player_input(&input, control, &baseline_observation).await;
        baseline_trace::finish_player_contribution(baseline_observation, &result);
        result.map(|player_contribution| BaselineOutput { player_contribution })
    }
}

impl BaselinePipeline {
    async fn process_player_input(
        &self,
        input: &BaselineInput,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<PlayerContribution, PipelineError> {
        let rendered_prompt =
            baseline_prompt::process_player_input(self.prompt.as_ref(), &input.player_input, observation)?;

        let player_contribution = baseline_llm::process_player_input(
            self.gateway.as_ref(),
            &input.player_input,
            rendered_prompt.into_messages(),
            control,
            observation,
        )
        .await?;

        Ok(player_contribution)
    }
}
