use crate::core::{BoundedText, PlayerContribution, StoryContext, TurnControl};
use crate::llm::LlmGateway;
use crate::persistence::Store;
use crate::pipeline::common::{Pipeline, PipelineError, PipelineStage};
use crate::pipeline::generate::{generate_llm, generate_prompt, generate_trace};
use crate::prompt::Prompt;
use crate::trace::Observation;
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
pub struct GenerateInput {
    pub story_goal: String,
    pub player_contribution: PlayerContribution,
    pub constraints: Vec<BoundedText>,
}

#[derive(Serialize)]
pub struct GenerateOutput {
    pub result: String,
    pub player_contribution: PlayerContribution,
}

pub struct GeneratePipeline {
    gateway: Arc<LlmGateway>,
    prompt: Arc<Prompt>,
    store: Arc<Store>,
}

impl GeneratePipeline {
    pub fn new(gateway: Arc<LlmGateway>, prompt: Arc<Prompt>, store: Arc<Store>) -> Self {
        Self { gateway, prompt, store }
    }

    async fn generate_story(
        &self,
        input: &GenerateInput,
        story_ctx: &StoryContext,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<String, PipelineError> {
        let rendered_prompt =
            generate_prompt::generate_story(self.prompt.as_ref(), story_ctx, input, self.store.as_ref(), observation)
                .await?;

        generate_llm::generate_story(self.gateway.as_ref(), rendered_prompt.into_messages(), control, observation)
            .await
            .map_err(PipelineError::llm(PipelineStage::Generate))
    }
}

impl Pipeline for GeneratePipeline {
    type Input = GenerateInput;
    type Output = GenerateOutput;

    fn stage(&self) -> PipelineStage {
        PipelineStage::Generate
    }

    async fn execute(
        &self,
        input: Self::Input,
        story_ctx: &StoryContext,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<Self::Output, PipelineError> {
        let generate_observation = generate_trace::begin_story_generation(
            observation,
            &input.story_goal,
            &input.player_contribution.processed,
        );
        let result = self.generate_story(&input, story_ctx, control, &generate_observation).await;
        generate_trace::finish_story_generation(generate_observation, &result);
        result.map(|result| GenerateOutput {
            result,
            player_contribution: input.player_contribution,
        })
    }
}
