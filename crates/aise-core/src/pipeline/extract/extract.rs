use crate::core::{StoryContext, TurnControl, TurnEventSink};
use crate::pipeline::common::{Pipeline, PipelineError, PipelineStage};
use crate::trace::Observation;
use serde::Serialize;

#[derive(Serialize)]
pub struct ExtractInput {
    pub query: String,
}

#[derive(Serialize)]
pub struct ExtractOutput {
    pub result: String,
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
        story_ctx: &StoryContext,
        control: &TurnControl,
        sink: &dyn TurnEventSink,
        observation: &Observation,
    ) -> Result<Self::Output, PipelineError> {
        Ok(ExtractOutput { result: input.query })
    }
}
