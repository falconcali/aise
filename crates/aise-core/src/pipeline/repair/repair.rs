use crate::core::{StoryContext, TurnControl};
use crate::pipeline::common::{Pipeline, PipelineError, PipelineStage, ValidationContext, ValidationIssue};
use crate::pipeline::repair::RepairConfig;
use crate::trace::Observation;
use serde::Serialize;

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
    config: RepairConfig,
}

impl RepairPipeline {
    pub fn new(config: RepairConfig) -> Self {
        Self { config }
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
        _story_ctx: &StoryContext,
        _control: &TurnControl,
        _observation: &Observation,
    ) -> Result<Self::Output, PipelineError> {
        if input.proposal.len() > self.config.max_proposal_bytes {
            return Err(PipelineError::invalid_output(
                PipelineStage::Repair,
                "repair_proposal_too_long",
                format!("repair proposal exceeds {} bytes", self.config.max_proposal_bytes),
            ));
        }
        Ok(RepairOutput {
            proposal: input.proposal.to_owned(),
        })
    }
}
