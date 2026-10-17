use crate::core::{StoryContext, TurnControl};
use crate::llm::LlmGateway;
use crate::pipeline::common::{
    Pipeline, PipelineError, PipelineStage, ProposalQualityPolicy, ValidationContext, ValidationReport,
};
use crate::pipeline::validate::{ValidateConfig, validate_llm, validate_prompt, validate_report, validate_trace};
use crate::prompt::Prompt;
use crate::trace::Observation;
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
pub struct ValidateInput<'a> {
    pub context: ValidationContext<'a>,
    pub proposal: &'a str,
    pub proposal_version: u32,
    pub repair_rounds_used: u32,
    pub repair_rounds_remaining: u32,
}

#[derive(Serialize)]
pub struct ValidateOutput {
    pub report: ValidationReport,
}

pub struct ValidatePipeline {
    gateway: Arc<LlmGateway>,
    prompt: Arc<Prompt>,
    config: ValidateConfig,
    quality_policy: Arc<ProposalQualityPolicy>,
}

impl ValidatePipeline {
    pub fn new(
        gateway: Arc<LlmGateway>,
        prompt: Arc<Prompt>,
        config: ValidateConfig,
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

    async fn evaluate(
        &self,
        input: &ValidateInput<'_>,
        story_ctx: &StoryContext,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<ValidationReport, PipelineError> {
        let rendered_prompt = validate_prompt::evaluate_story(
            self.prompt.as_ref(),
            story_ctx,
            input,
            self.quality_policy.as_ref(),
            self.config.max_issue_bytes,
            observation,
        )?;
        let raw =
            validate_llm::evaluate_story(self.gateway.as_ref(), rendered_prompt.into_messages(), control, observation)
                .await
                .map_err(PipelineError::llm(PipelineStage::Validate))?;
        validate_report::parse_validation_report(
            &raw,
            input.proposal_version,
            self.quality_policy.as_ref(),
            self.config.max_issue_bytes,
        )
    }
}

impl<'a> Pipeline<ValidateInput<'a>> for ValidatePipeline {
    type Output = ValidateOutput;

    fn stage(&self) -> PipelineStage {
        PipelineStage::Validate
    }

    async fn execute(
        &self,
        input: ValidateInput<'a>,
        story_ctx: &StoryContext,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<Self::Output, PipelineError> {
        let evaluator_observation = validate_trace::begin_evaluator(observation, &input);
        let result = self.evaluate(&input, story_ctx, control, &evaluator_observation).await;
        validate_trace::finish_evaluator(evaluator_observation, &result);
        result.map(|report| ValidateOutput { report })
    }
}

#[cfg(test)]
#[path = "tests/validate_tests.rs"]
mod tests;
