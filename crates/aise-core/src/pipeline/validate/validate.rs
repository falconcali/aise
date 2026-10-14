use crate::core::{BoundedText, PlayerContribution, StoryContext, TurnControl};
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
pub struct ValidateInput<'a> {
    pub original_proposal: &'a str,
    pub current_proposal: &'a str,
    pub proposal_version: u32,
    pub constraints: &'a [BoundedText],

    pub player_contribution: &'a PlayerContribution,
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

#[derive(Serialize)]
pub struct ValidateOutput {
    pub scores: Vec<ValidateScoreResult>,
    pub decision: ValidationDecision,
}

impl ValidateOutput {
    pub fn total_score(&self) -> f32 {
        self.scores.iter().map(|score| score.score).sum()
    }
}

pub struct ValidatePipeline {
    pub score_configs: Vec<ValidateScoreConfig>,
}

impl<'a> Pipeline<ValidateInput<'a>> for ValidatePipeline {
    type Output = ValidateOutput;

    fn stage(&self) -> PipelineStage {
        PipelineStage::Validate
    }

    async fn execute(
        &self,
        _input: ValidateInput<'a>,
        _story_ctx: &StoryContext,
        _control: &TurnControl,
        _observation: &Observation,
    ) -> Result<Self::Output, PipelineError> {
        Ok(ValidateOutput {
            decision: ValidationDecision::Accept,
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
