use crate::core::{PlayerContribution, StoryContext, TurnControl, TurnEventSink};
use crate::llm::LlmGateway;
use crate::persistence::StoryStore;
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
    story_store: Arc<dyn StoryStore>,
}

impl BaselinePipeline {
    pub fn new(gateway: Arc<LlmGateway>, prompt: Arc<Prompt>, story_store: Arc<dyn StoryStore>) -> Self {
        Self {
            gateway,
            prompt,
            story_store,
        }
    }

    async fn process_player_input(
        &self,
        input: &BaselineInput,
        story_ctx: &StoryContext,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<PlayerContribution, PipelineError> {
        let rendered_prompt = baseline_prompt::process_player_input(
            self.prompt.as_ref(),
            story_ctx,
            &input.player_input,
            self.story_store.as_ref(),
            observation,
        )
        .await?;

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

impl Pipeline for BaselinePipeline {
    type Input = BaselineInput;
    type Output = BaselineOutput;

    fn stage(&self) -> PipelineStage {
        PipelineStage::Baseline
    }

    async fn execute(
        &self,
        input: Self::Input,
        story_ctx: &StoryContext,
        control: &TurnControl,
        sink: &dyn TurnEventSink,
        observation: &Observation,
    ) -> Result<Self::Output, PipelineError> {
        let baseline_observation = baseline_trace::begin_player_contribution(observation, &input.player_input);
        let result = self
            .process_player_input(&input, story_ctx, control, &baseline_observation)
            .await;
        baseline_trace::finish_player_contribution(baseline_observation, &result);
        result.map(|player_contribution: PlayerContribution| BaselineOutput { player_contribution })
    }
}
