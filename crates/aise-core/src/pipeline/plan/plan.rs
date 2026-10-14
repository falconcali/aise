use crate::core::{
    ActivatedKnowledge, BoundedText, FactId, NarrativeDirection, PlayerContribution, RumorId, StoryContext, TurnControl,
};
use crate::llm::LlmGateway;
use crate::persistence::Store;
use crate::pipeline::common::{Pipeline, PipelineError, PipelineStage};
use crate::pipeline::plan::{
    PlanConfig, plan_candidates, plan_llm, plan_output, plan_prompt,
    plan_trace::{self, PlanTraceMetrics},
};
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
        metrics: &mut PlanTraceMetrics,
    ) -> Result<PlanOutput, PipelineError> {
        let story_pack = self
            .store
            .pack_store()
            .get_pack(&story_ctx.pack_ref)
            .await
            .map_err(PipelineError::persistence(PipelineStage::Plan))?;
        let candidates = plan_candidates::collect_candidates(
            story_pack.as_ref(),
            input.knowledge,
            self.config.max_retrieval_candidates_per_kind,
        );
        metrics.fact_candidates_total = candidates.fact_stats.total;
        metrics.fact_candidates_offered = candidates.fact_stats.offered;
        metrics.rumor_candidates_total = candidates.rumor_stats.total;
        metrics.rumor_candidates_offered = candidates.rumor_stats.offered;
        if candidates.fact_stats.truncated() || candidates.rumor_stats.truncated() {
            plan_trace::warn_if_candidates_truncated(metrics);
        }
        let rendered_prompt = plan_prompt::process_plan(
            self.prompt.as_ref(),
            story_ctx,
            input,
            story_pack.as_ref(),
            &candidates,
            observation,
        )?;
        let raw_output =
            plan_llm::process_plan(self.gateway.as_ref(), rendered_prompt.into_messages(), control, observation)
                .await
                .map_err(PipelineError::llm(PipelineStage::Plan))?;
        let output = plan_output::parse_plan_output(&raw_output, &candidates, &self.config)?;
        metrics.facts_selected = output.facts_to_retrieve.len();
        metrics.rumors_selected = output.rumors_to_retrieve.len();
        Ok(output)
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
        let mut metrics = PlanTraceMetrics::default();
        let result = self
            .process_story_plan(story_ctx, &input, control, &plan_observation, &mut metrics)
            .await;
        plan_trace::finish_plan(plan_observation, &result, &metrics);
        result
    }
}

#[cfg(test)]
#[path = "tests/plan_tests.rs"]
mod tests;
