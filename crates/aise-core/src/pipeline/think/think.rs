use crate::core::{
    CharacterDecision, CharacterThinkRequest, PlayerContribution, RoleEntry, RumorEntry, StoryContext, TurnControl,
};
use crate::llm::LlmGateway;
use crate::pipeline::common::{Pipeline, PipelineError, PipelineStage};
use crate::pipeline::think::{ThinkConfig, think_llm, think_output, think_prompt, think_trace};
use crate::prompt::Prompt;
use crate::trace::Observation;
use futures::{FutureExt, StreamExt, TryStreamExt, stream};
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Arc;

#[derive(Serialize)]
pub struct ThinkInput<'a> {
    pub requests: &'a [CharacterThinkRequest],
    pub player_contribution: &'a PlayerContribution,
    pub activated_rumors: &'a [RumorEntry],
    pub retrieved_rumors: &'a [RumorEntry],
}

#[derive(Default, Serialize)]
pub struct ThinkOutput {
    pub decisions: Vec<CharacterDecision>,
}

pub struct ThinkPipeline {
    gateway: Arc<LlmGateway>,
    prompt: Arc<Prompt>,
    config: ThinkConfig,
}

impl ThinkPipeline {
    pub fn new(gateway: Arc<LlmGateway>, prompt: Arc<Prompt>, config: ThinkConfig) -> Self {
        Self {
            gateway,
            prompt,
            config,
        }
    }

    async fn think(
        &self,
        input: &ThinkInput<'_>,
        story_ctx: &StoryContext,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<ThinkOutput, PipelineError> {
        validate_input(input, story_ctx)?;
        let decisions = stream::iter(0..input.requests.len())
            .map(|request_index| {
                async move {
                    let request = &input.requests[request_index];
                    let role = story_ctx.roles.get(&request.role_id).ok_or_else(|| {
                        inconsistent(
                            "think_resolved_role_missing",
                            format!("role ID {} is missing from the story context", request.role_id),
                        )
                    })?;
                    self.decide(request, role, story_ctx, input, control, observation).await
                }
                .boxed()
            })
            .buffered(self.config.max_concurrency.get())
            .try_collect()
            .await?;
        Ok(ThinkOutput { decisions })
    }

    async fn decide(
        &self,
        request: &CharacterThinkRequest,
        role: &RoleEntry,
        story_ctx: &StoryContext,
        input: &ThinkInput<'_>,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<CharacterDecision, PipelineError> {
        let role_observation = think_trace::begin_role(observation, &request.role_id);
        let result = self
            .decide_role(request, role, story_ctx, input, control, &role_observation)
            .await;
        think_trace::finish_role(role_observation, &result);
        result
    }

    async fn decide_role(
        &self,
        request: &CharacterThinkRequest,
        role: &RoleEntry,
        story_ctx: &StoryContext,
        input: &ThinkInput<'_>,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<CharacterDecision, PipelineError> {
        let rendered = think_prompt::character_decision(
            self.prompt.as_ref(),
            think_prompt::CharacterDecisionPrompt {
                story_ctx,
                input,
                request,
                role,
                config: &self.config,
            },
            observation,
        )?;
        let raw = think_llm::character_decision(self.gateway.as_ref(), rendered.into_messages(), control, observation)
            .await
            .map_err(PipelineError::llm(PipelineStage::Think))?;
        think_output::parse_character_decision(&raw, &request.role_id, &self.config)
    }
}

impl<'a> Pipeline<ThinkInput<'a>> for ThinkPipeline {
    type Output = ThinkOutput;

    fn stage(&self) -> PipelineStage {
        PipelineStage::Think
    }

    async fn execute(
        &self,
        input: ThinkInput<'a>,
        story_ctx: &StoryContext,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<Self::Output, PipelineError> {
        let think_observation = think_trace::begin_think(observation);
        let result = self.think(&input, story_ctx, control, &think_observation).await;
        think_trace::finish_think(think_observation, &result);
        result
    }
}

fn validate_input(input: &ThinkInput<'_>, story_ctx: &StoryContext) -> Result<(), PipelineError> {
    let mut request_roles = HashSet::with_capacity(input.requests.len());
    for request in input.requests {
        if !request_roles.insert(&request.role_id) {
            return Err(inconsistent(
                "think_duplicate_request_role_id",
                format!("duplicate Character Think request for role {}", request.role_id),
            ));
        }
        if request.role_id == story_ctx.player {
            return Err(inconsistent(
                "think_player_role_forbidden",
                format!("Character Think request targets player role {}", request.role_id),
            ));
        }
        story_ctx.roles.get(&request.role_id).ok_or_else(|| {
            inconsistent(
                "think_resolved_role_missing",
                format!("role ID {} is missing from the story context", request.role_id),
            )
        })?;
    }
    Ok(())
}

fn inconsistent(code: &'static str, message: impl Into<String>) -> PipelineError {
    PipelineError::inconsistent_data(PipelineStage::Think, code, message)
}

#[cfg(test)]
#[path = "tests/think_tests.rs"]
mod tests;
