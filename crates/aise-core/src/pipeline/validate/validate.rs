use crate::core::{StoryContext, TurnControl, TurnEventSink};
use crate::pipeline::common::{Pipeline, PipelineError, PipelineStage};
use crate::trace::Observation;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ValidationDecision {
    Accept,
    Reject,
    Repair,
}

#[derive(Serialize)]
pub struct ValidateInput {
    pub original_proposal: String,
    pub current_proposal: String,
    pub proposal_version: u32,
}

#[derive(Clone, Serialize)]
pub struct ValidateScoreConfig {
    pub name: String,
    pub description: String,

    pub target_score: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidateScoreResult {
    pub name: String,
    pub description: String,

    pub score: f32,
    pub target_score: f32,
    pub issue: String,
}

#[derive(Clone, Serialize)]
pub struct ValidateOutput {
    pub original_proposal: String,
    pub current_proposal: String,
    pub scores: Vec<ValidateScoreResult>,
    pub proposal_version: u32,
    pub decision: ValidationDecision,
}

pub struct ValidatePipeline {
    pub score_configs: Vec<ValidateScoreConfig>,
    pub validation_budget: u32,
}

impl Pipeline for ValidatePipeline {
    type Input = ValidateInput;
    type Output = ValidateOutput;

    fn stage(&self) -> PipelineStage {
        PipelineStage::Validate
    }

    async fn execute(
        &self,
        story_ctx: &StoryContext,
        input: Self::Input,
        control: &TurnControl,
        sink: &dyn TurnEventSink,
        observation: &Observation,
    ) -> Result<Self::Output, PipelineError> {
        Ok(ValidateOutput {
            decision: ValidationDecision::Accept,
            original_proposal: input.original_proposal,
            current_proposal: input.current_proposal,
            proposal_version: input.proposal_version,
            scores: self
                .score_configs
                .iter()
                .map(|config| ValidateScoreResult {
                    name: config.name.clone(),
                    description: config.description.clone(),
                    score: 10.0,
                    target_score: config.target_score,
                    issue: "Not so good".to_string(),
                })
                .collect(),
        })
    }
}
