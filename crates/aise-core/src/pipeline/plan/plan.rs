use crate::core::{
    ActivatedKnowledge, BoundedText, FactId, NarrativeDirection, PlayerContribution, RumorId, StoryContext, TurnControl,
};
use crate::llm::LlmGateway;
use crate::persistence::Store;
use crate::pipeline::common::{Pipeline, PipelineError, PipelineStage};
use crate::pipeline::plan::{PlanConfig, plan_llm, plan_prompt, plan_trace};
use crate::prompt::Prompt;
use crate::trace::Observation;
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
pub struct PlanInput<'a> {
    pub player_contribution: &'a PlayerContribution,
    pub directions: &'a [NarrativeDirection],
    pub constraints: &'a [BoundedText],
    pub knowledge: &'a ActivatedKnowledge,
}

#[derive(Serialize)]
pub struct PlanOutput {
    pub story_goal: BoundedText,
    pub facts_to_retrieve: Vec<FactId>,
    pub rumors_to_retrieve: Vec<RumorId>,
}

impl PlanOutput {
    pub fn requires_retrieval(&self) -> bool {
        !self.facts_to_retrieve.is_empty() || !self.rumors_to_retrieve.is_empty()
    }

    pub fn requires_character_thinking(&self) -> bool {
        false
    }
}

pub struct PlanPipeline {
    gateway: Arc<LlmGateway>,
    prompt: Arc<Prompt>,
    store: Arc<Store>,
    config: PlanConfig,
}

impl PlanPipeline {
    pub fn new(gateway: Arc<LlmGateway>, prompt: Arc<Prompt>, store: Arc<Store>, config: PlanConfig) -> Self {
        Self {
            gateway,
            prompt,
            store,
            config,
        }
    }

    pub async fn process_story_plan(
        &self,
        story_ctx: &StoryContext,
        input: &PlanInput<'_>,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<String, PipelineError> {
        let rendered_prompt =
            plan_prompt::process_plan(self.prompt.as_ref(), story_ctx, input, self.store.as_ref(), observation).await?;

        let story_plan =
            plan_llm::process_plan(self.gateway.as_ref(), rendered_prompt.into_messages(), control, observation)
                .await
                .map_err(PipelineError::llm(PipelineStage::Plan))?;

        Ok(story_plan)
    }
}

impl<'a> Pipeline<PlanInput<'a>> for PlanPipeline {
    type Output = PlanOutput;

    fn stage(&self) -> PipelineStage {
        PipelineStage::Plan
    }

    async fn execute(
        &self,
        input: PlanInput<'a>,
        story_ctx: &StoryContext,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<Self::Output, PipelineError> {
        let plan_observation = plan_trace::begin_plan(observation, &input.player_contribution.processed);
        let result = self.process_story_plan(story_ctx, &input, control, &plan_observation).await;
        plan_trace::finish_plan(plan_observation, &result);
        result.and_then(|plan| {
            let story_goal =
                BoundedText::try_new(plan, "story_goal", self.config.max_story_goal_bytes).map_err(|error| {
                    PipelineError::invalid_output(PipelineStage::Plan, "plan_story_goal_too_long", error.to_string())
                })?;
            Ok(PlanOutput {
                story_goal,
                facts_to_retrieve: Vec::new(),
                rumors_to_retrieve: Vec::new(),
            })
        })
    }
}

#[cfg(test)]
#[path = "tests/plan_tests.rs"]
mod tests;
