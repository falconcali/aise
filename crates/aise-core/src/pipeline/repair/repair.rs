use crate::core::{StoryContext, TurnControl};
use crate::llm::LlmGateway;
use crate::pipeline::common::{
    Pipeline, PipelineError, PipelineStage, ProposalQualityPolicy, ValidationContext, ValidationIssue,
};
use crate::pipeline::repair::{RepairConfig, repair_llm, repair_output, repair_prompt, repair_trace};
use crate::prompt::Prompt;
use crate::trace::Observation;
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
pub struct RepairInput<'a> {
    pub context: ValidationContext<'a>,
    pub proposal: &'a str,
    pub proposal_version: u32,
    pub issues: &'a [ValidationIssue],
    pub repair_round: u32,
    pub repair_rounds_remaining: u32,
}

#[derive(Serialize)]
pub struct RepairOutput {
    pub proposal: String,
}

pub struct RepairPipeline {
    gateway: Arc<LlmGateway>,
    prompt: Arc<Prompt>,
    config: RepairConfig,
    quality_policy: Arc<ProposalQualityPolicy>,
}

impl RepairPipeline {
    pub fn new(
        gateway: Arc<LlmGateway>,
        prompt: Arc<Prompt>,
        config: RepairConfig,
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

    async fn repair(
        &self,
        input: &RepairInput<'_>,
        story_ctx: &StoryContext,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<String, PipelineError> {
        let rendered_prompt = repair_prompt::repair_story(
            self.prompt.as_ref(),
            story_ctx,
            input,
            self.quality_policy.as_ref(),
            &self.config,
            observation,
        )?;
        let raw =
            repair_llm::repair_story(self.gateway.as_ref(), rendered_prompt.into_messages(), control, observation)
                .await
                .map_err(PipelineError::llm(PipelineStage::Repair))?;
        repair_output::validate_repaired_proposal(raw, &self.config)
    }
}

impl<'a> Pipeline<RepairInput<'a>> for RepairPipeline {
    type Output = RepairOutput;

    fn stage(&self) -> PipelineStage {
        PipelineStage::Repair
    }

    async fn execute(
        &self,
        input: RepairInput<'a>,
        story_ctx: &StoryContext,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<Self::Output, PipelineError> {
        let repair_observation = repair_trace::begin_repair(observation, &input);
        let result = self.repair(&input, story_ctx, control, &repair_observation).await;
        repair_trace::finish_repair(repair_observation, &result);
        result.map(|proposal| RepairOutput { proposal })
    }
}

#[cfg(test)]
#[path = "tests/repair_tests.rs"]
mod tests;
