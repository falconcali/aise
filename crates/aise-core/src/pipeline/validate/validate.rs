use crate::core::{StoryContext, TurnControl};
use crate::pipeline::common::{
    Pipeline, PipelineError, PipelineStage, ValidationCheckResult, ValidationContext, ValidationReport,
    ValidationScoreResult,
};
use crate::pipeline::validate::ValidateConfig;
use crate::trace::Observation;
use serde::Serialize;

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
    config: ValidateConfig,
}

impl ValidatePipeline {
    pub fn new(config: ValidateConfig) -> Self {
        Self { config }
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
        _story_ctx: &StoryContext,
        _control: &TurnControl,
        _observation: &Observation,
    ) -> Result<Self::Output, PipelineError> {
        Ok(ValidateOutput {
            report: ValidationReport {
                proposal_version: input.proposal_version,
                checks: self
                    .config
                    .checks
                    .iter()
                    .map(|config| ValidationCheckResult {
                        name: config.name.clone(),
                        description: config.description.clone(),
                        passed: true,
                        issue: None,
                    })
                    .collect(),
                scores: self
                    .config
                    .scores
                    .iter()
                    .map(|config| ValidationScoreResult {
                        name: config.name.clone(),
                        description: config.description.clone(),
                        min_score: config.min_score,
                        max_score: config.max_score,
                        target_score: config.target_score,
                        score: config.target_score,
                        issue: None,
                    })
                    .collect(),
            },
        })
    }
}
