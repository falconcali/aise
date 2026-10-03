use crate::core::{TurnControl, TurnEventSink};
use crate::pipeline::common::{Pipeline, PipelineError, PipelineStage};
use crate::trace::Observation;

pub struct ThinkInput {
    pub query: String,
}

pub struct ThinkOutput {
    pub result: String,
}

pub struct ThinkPipeline;

impl Pipeline for ThinkPipeline {
    type Input = ThinkInput;
    type Output = ThinkOutput;

    fn stage(&self) -> PipelineStage {
        PipelineStage::Think
    }

    async fn execute(
        &self,
        input: Self::Input,
        control: &TurnControl,
        sink: &dyn TurnEventSink,
        observation: &Observation,
    ) -> Result<Self::Output, PipelineError> {
        Ok(ThinkOutput { result: input.query })
    }
}
