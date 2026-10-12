use crate::core::{PlayerContribution, StoryContext, TurnControl, TurnEventSink};
use crate::llm::LlmGateway;
use crate::persistence::Store;
use crate::pipeline::baseline::baseline_narrative::{self, NarrativeProjection};
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
    pub narrative: NarrativeProjection,
}

pub struct BaselinePipeline {
    gateway: Arc<LlmGateway>,
    prompt: Arc<Prompt>,
    store: Arc<Store>,
}

impl BaselinePipeline {
    pub fn new(gateway: Arc<LlmGateway>, prompt: Arc<Prompt>, store: Arc<Store>) -> Self {
        Self { gateway, prompt, store }
    }

    async fn project_narrative(&self, story_ctx: &StoryContext) -> Result<NarrativeProjection, PipelineError> {
        let story_pack = self
            .store
            .pack_store()
            .get_pack(&story_ctx.pack_ref)
            .await
            .map_err(|error| PipelineError::new(PipelineStage::Baseline, error.to_string()))?;
        baseline_narrative::project_narrative(&story_pack, story_ctx)
            .map_err(|error| PipelineError::new(PipelineStage::Baseline, error.to_string()))
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
            self.store.as_ref(),
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
        let narrative = self.project_narrative(story_ctx).await?;

        let baseline_observation = baseline_trace::begin_player_contribution(observation, &input.player_input);
        let result = self
            .process_player_input(&input, story_ctx, control, &baseline_observation)
            .await;
        baseline_trace::finish_player_contribution(baseline_observation, &result);
        result.map(|player_contribution: PlayerContribution| BaselineOutput {
            player_contribution,
            narrative,
        })
    }
}
