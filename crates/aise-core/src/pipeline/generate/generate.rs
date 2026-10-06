use crate::core::{StoryContext, TurnControl, TurnEventSink};
use crate::pipeline::common::{Pipeline, PipelineError, PipelineStage};
use crate::trace::Observation;
use serde::Serialize;

#[derive(Serialize)]
pub struct GenerateInput {
    pub query: String,
}

#[derive(Serialize)]
pub struct GenerateOutput {
    pub result: String,
}

pub struct GeneratePipeline;

impl Pipeline for GeneratePipeline {
    type Input = GenerateInput;
    type Output = GenerateOutput;

    fn stage(&self) -> PipelineStage {
        PipelineStage::Generate
    }

    async fn execute(
        &self,
        input: Self::Input,
        story_ctx: &StoryContext,
        control: &TurnControl,
        sink: &dyn TurnEventSink,
        observation: &Observation,
    ) -> Result<Self::Output, PipelineError> {
        Ok(GenerateOutput { result: input.query })
    }
}
