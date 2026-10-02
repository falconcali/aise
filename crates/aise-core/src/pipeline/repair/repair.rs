use crate::pipeline::common::{Pipeline, PipelineStage, PipelineError, ValidateScoreResult};
use crate::core::{TurnControl, TurnEventSink};
use crate::trace::Observation;

pub struct RepairInput {
    pub query: String,
    pub scores: Vec<ValidateScoreResult>
}

pub struct RepairOutput {
    pub result: String
}

pub struct RepairPipeline;

impl Pipeline for RepairPipeline {
    type Input = RepairInput;
    type Output = RepairOutput;

    fn stage(&self) -> PipelineStage {
        PipelineStage::Repair
    }

    async fn execute(
        &self,
        input: Self::Input,
        control: &TurnControl,
        sink: &dyn TurnEventSink,
        observation: &Observation
    ) -> Result<Self::Output, PipelineError> {
        Ok(RepairOutput { result: input.query })
    }
}