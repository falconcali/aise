use crate::core::{
    ActivatedKnowledge, BoundedText, CharacterThinkRequest, FactId, NarrativeDirection, PlayerContribution,
    RoleId, RumorId, StoryContext, TurnControl,
};
use crate::llm::LlmGateway;
use crate::pipeline::common::{Pipeline, PipelineError, PipelineStage};
use crate::pipeline::plan::{PlanConfig, plan_candidates, plan_llm, plan_prompt, plan_roles, plan_trace};
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

#[derive(Debug, Serialize)]
pub struct PlanOutput {
    pub story_goal: BoundedText,
    pub facts_to_retrieve: Vec<FactId>,
    pub rumors_to_retrieve: Vec<RumorId>,
    pub roles_for_generation: Vec<RoleId>,
    pub character_think_requests: Vec<CharacterThinkRequest>,
}

impl PlanOutput {
    pub fn requires_retrieval(&self) -> bool {
        !self.facts_to_retrieve.is_empty() || !self.rumors_to_retrieve.is_empty()
    }

    pub fn requires_character_thinking(&self) -> bool {
        !self.character_think_requests.is_empty()
    }
}

pub struct PlanPipeline {
    gateway: Arc<LlmGateway>,
    prompt: Arc<Prompt>,
    config: PlanConfig,
}

impl PlanPipeline {
    pub fn new(gateway: Arc<LlmGateway>, prompt: Arc<Prompt>, config: PlanConfig) -> Self {
        Self {
            gateway,
            prompt,
            config,
        }
    }

    pub async fn process_story_plan(
        &self,
        story_ctx: &StoryContext,
        input: &PlanInput<'_>,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<PlanOutput, PipelineError> {
        let candidates = plan_candidates::collect_candidates(
            story_ctx,
            input.knowledge,
            self.config.max_retrieval_candidates_per_kind,
        );
        let player_role = plan_roles::resolve_player_role(story_ctx)?;
        let role_catalog = plan_roles::collect_role_catalog(story_ctx, &story_ctx.player);
        let rendered_prompt = plan_prompt::process_plan(
            self.prompt.as_ref(),
            plan_prompt::PlanPromptContext {
                story_ctx,
                input,
                candidates: &candidates,
                player_role: &player_role,
                role_catalog: &role_catalog,
                config: &self.config,
            },
            observation,
        )?;
        let raw_output =
            plan_llm::process_plan(self.gateway.as_ref(), rendered_prompt.into_messages(), control, observation)
                .await
                .map_err(PipelineError::llm(PipelineStage::Plan))?;
        let output =
            plan_llm::parse_plan_output(&raw_output, &candidates, &role_catalog, &player_role.id, &self.config)?;
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
        let result = self.process_story_plan(story_ctx, &input, control, &plan_observation).await;
        plan_trace::finish_plan(plan_observation, &result);
        result
    }
}

#[cfg(test)]
#[path = "tests/plan_tests.rs"]
mod tests;
