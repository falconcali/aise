use crate::core::{
    ActivatedKnowledge, BoundedText, CharacterDecision, FactEntry, NarrativeDirection, PlayerContribution, RoleId,
    RumorEntry, StoryContext, TurnControl,
};
use crate::llm::LlmGateway;
use crate::pipeline::common::{Pipeline, PipelineError, PipelineStage, ProposalQualityPolicy};
use crate::pipeline::generate::{GenerateConfig, generate_llm, generate_prompt, generate_roles, generate_trace};
use crate::prompt::Prompt;
use crate::trace::Observation;
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
pub struct GenerateInput<'a> {
    pub story_goal: &'a str,
    pub player_contribution: &'a PlayerContribution,
    pub directions: &'a [NarrativeDirection],
    pub constraints: &'a [BoundedText],
    pub knowledge: &'a ActivatedKnowledge,
    pub retrieved_facts: &'a [FactEntry],
    pub retrieved_rumors: &'a [RumorEntry],
    pub role_ids: &'a [RoleId],
    pub character_decisions: &'a [CharacterDecision],
}

#[derive(Serialize)]
pub struct GenerateOutput {
    pub result: String,
}

pub struct GeneratePipeline {
    gateway: Arc<LlmGateway>,
    prompt: Arc<Prompt>,
    config: GenerateConfig,
    quality_policy: Arc<ProposalQualityPolicy>,
}

impl GeneratePipeline {
    pub fn new(
        gateway: Arc<LlmGateway>,
        prompt: Arc<Prompt>,
        config: GenerateConfig,
        quality_policy: Arc<ProposalQualityPolicy>,
    ) -> Self {
        Self {
            gateway,
            prompt,
            config,
            quality_policy,
        }
    }

    #[cfg(test)]
    pub(in crate::pipeline) fn quality_policy(&self) -> &Arc<ProposalQualityPolicy> {
        &self.quality_policy
    }

    async fn generate_story(
        &self,
        input: &GenerateInput<'_>,
        story_ctx: &StoryContext,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<String, PipelineError> {
        let role_context =
            generate_roles::project_role_context(story_ctx, input.role_ids, input.character_decisions, &self.config)?;
        let rendered_prompt = generate_prompt::generate_story(
            self.prompt.as_ref(),
            story_ctx,
            input,
            &role_context,
            self.quality_policy.as_ref(),
            observation,
        )?;

        generate_llm::generate_story(self.gateway.as_ref(), rendered_prompt.into_messages(), control, observation)
            .await
            .map_err(PipelineError::llm(PipelineStage::Generate))
    }
}

impl<'a> Pipeline<GenerateInput<'a>> for GeneratePipeline {
    type Output = GenerateOutput;

    fn stage(&self) -> PipelineStage {
        PipelineStage::Generate
    }

    async fn execute(
        &self,
        input: GenerateInput<'a>,
        story_ctx: &StoryContext,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<Self::Output, PipelineError> {
        let generate_observation =
            generate_trace::begin_story_generation(observation, input.story_goal, &input.player_contribution.processed);
        let result = self.generate_story(&input, story_ctx, control, &generate_observation).await;
        generate_trace::finish_story_generation(generate_observation, &result);
        result.map(|result| GenerateOutput { result })
    }
}

#[cfg(test)]
#[path = "tests/generate_tests.rs"]
mod tests;
