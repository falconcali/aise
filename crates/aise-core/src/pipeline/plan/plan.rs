use crate::core::{BoundedText, NarrativeDirection, PlayerContribution, StoryContext, TurnControl, TurnEventSink};
use crate::llm::LlmGateway;
use crate::persistence::Store;
use crate::pipeline::common::{Pipeline, PipelineError, PipelineStage};
use crate::pipeline::plan::{plan_llm, plan_prompt, plan_trace};
use crate::prompt::Prompt;
use crate::trace::Observation;
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
pub struct PlanInput {
    pub player_contribution: PlayerContribution,
    pub directions: Vec<NarrativeDirection>,
    pub constraints: Vec<BoundedText>,
}

#[derive(Serialize)]
pub struct PlanOutput {
    pub plan: String,
    pub player_contribution: PlayerContribution,
}

impl PlanOutput {
    pub fn requires_retrieval(&self) -> bool {
        return false;
    }

    pub fn requires_character_thinking(&self) -> bool {
        return false;
    }
}

pub struct PlanPipeline {
    gateway: Arc<LlmGateway>,
    prompt: Arc<Prompt>,
    store: Arc<Store>,
}

impl PlanPipeline {
    pub fn new(gateway: Arc<LlmGateway>, prompt: Arc<Prompt>, store: Arc<Store>) -> Self {
        Self { gateway, prompt, store }
    }

    pub async fn process_story_plan(
        &self,
        story_ctx: &StoryContext,
        input: &PlanInput,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<String, PipelineError> {
        let rendered_prompt =
            plan_prompt::process_plan(self.prompt.as_ref(), story_ctx, input, self.store.as_ref(), observation).await?;

        let story_plan =
            plan_llm::process_plan(self.gateway.as_ref(), rendered_prompt.into_messages(), control, observation)
                .await?;

        Ok(story_plan)
    }
}

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
        let plan_observation = plan_trace::begin_plan(observation, &input.player_contribution.processed);
        let result = self.process_story_plan(story_ctx, &input, control, &plan_observation).await;
        plan_trace::finish_plan(plan_observation, &result);
        result.map(|plan| PlanOutput {
            plan,
            player_contribution: input.player_contribution,
        })
    }
}
