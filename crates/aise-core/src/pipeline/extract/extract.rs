use crate::pipeline::common::{Pipeline, PipelineStage, PipelineError};
use crate::core::{TurnControl, TurnEventSink};
use crate::trace::Observation;

pub struct ExtractInput {
    pub query: String
}

pub struct ExtractOutput {
    pub result: String
}

pub struct ExtractPipeline;

impl Pipeline for ExtractPipeline {
    type Input = ExtractInput;
    type Output = ExtractOutput;

    fn stage(&self) -> PipelineStage {
        PipelineStage::Extract
    }

    async fn execute(
        &self,
        input: Self::Input,
        control: &TurnControl,
        sink: &dyn TurnEventSink,
        observation: &Observation
    ) -> Result<Self::Output, PipelineError> {
        Ok(ExtractOutput { result: input.query })
    }
}