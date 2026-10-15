use crate::core::{
    ActivatedKnowledge, BoundedText, CharacterDecision, FactEntry, NarrativeDirection, PlayerContribution,
    PlayerRoleContext, ResolvedRole, RumorEntry, StoryContext, TurnControl,
};
use crate::llm::LlmGateway;
use crate::persistence::Store;
use crate::pipeline::common::{Pipeline, PipelineError, PipelineStage};
use crate::pipeline::generate::{
    GenerateConfig, generate_llm, generate_prompt, generate_roles,
    generate_trace::{self, GenerateTraceMetrics},
};
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
    pub player_role: &'a PlayerRoleContext,
    pub roles: &'a [ResolvedRole],
    pub character_decisions: &'a [CharacterDecision],
}

#[derive(Serialize)]
pub struct GenerateOutput {
    pub result: String,
}

pub struct GeneratePipeline {
    gateway: Arc<LlmGateway>,
    prompt: Arc<Prompt>,
    store: Arc<Store>,
    config: GenerateConfig,
}

impl GeneratePipeline {
    pub fn new(gateway: Arc<LlmGateway>, prompt: Arc<Prompt>, store: Arc<Store>, config: GenerateConfig) -> Self {
        Self {
            gateway,
            prompt,
            store,
            config,
        }
    }

    async fn generate_story(
        &self,
        input: &GenerateInput<'_>,
        story_ctx: &StoryContext,
        control: &TurnControl,
        observation: &Observation,
        metrics: &mut GenerateTraceMetrics,
    ) -> Result<String, PipelineError> {
        generate_roles::validate_player_role(input.player_role, &story_ctx.player)?;
        let role_context = generate_roles::project_role_context(
            input.player_role,
            input.roles,
            input.character_decisions,
            &self.config,
        )?;
        metrics.role_context_bytes = role_context.player_role.len().saturating_add(role_context.roles.len());
        metrics.decision_context_bytes = role_context.decisions.len();
        let rendered_prompt = generate_prompt::generate_story(
            self.prompt.as_ref(),
            story_ctx,
            input,
            &role_context,
            self.store.as_ref(),
            observation,
        )
        .await?;

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
        let mut metrics = GenerateTraceMetrics {
            roles: input.roles.len(),
            character_decisions: input.character_decisions.len(),
            ..GenerateTraceMetrics::default()
        };
        let result = self
            .generate_story(&input, story_ctx, control, &generate_observation, &mut metrics)
            .await;
        generate_trace::finish_story_generation(generate_observation, &result, &metrics);
        result.map(|result| GenerateOutput { result })
    }
}
