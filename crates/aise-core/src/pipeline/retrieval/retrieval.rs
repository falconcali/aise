use crate::core::{TurnControl, TurnEventSink};
use crate::pipeline::common::{Pipeline, PipelineError, PipelineStage};
use crate::trace::Observation;

pub struct RetrievalInput {
    pub query: String,
}

pub struct RetrievalOutput {
    pub result: String,
}

pub struct RetrievalPipeline;

impl Pipeline for RetrievalPipeline {
    type Input = RetrievalInput;
    type Output = RetrievalOutput;

    fn stage(&self) -> PipelineStage {
        PipelineStage::Retrieval
    }

    async fn execute(
        &self,
        input: Self::Input,
        control: &TurnControl,
        sink: &dyn TurnEventSink,
        observation: &Observation,
    ) -> Result<Self::Output, PipelineError> {
        Ok(RetrievalOutput { result: input.query })
    }
}
