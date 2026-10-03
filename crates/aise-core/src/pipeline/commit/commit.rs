use crate::core::{TurnControl, TurnEventSink};
use crate::pipeline::common::{Pipeline, PipelineError, PipelineStage};
use crate::trace::Observation;
use serde::Serialize;

#[derive(Serialize)]
pub struct CommitInput {
    pub query: String,
}

#[derive(Serialize)]
pub struct CommitOutput {
    pub result: String,
}

pub struct CommitPipeline;

impl Pipeline for CommitPipeline {
    type Input = CommitInput;
    type Output = CommitOutput;

    fn stage(&self) -> PipelineStage {
        PipelineStage::Commit
    }

    async fn execute(
        &self,
        input: Self::Input,
        control: &TurnControl,
        sink: &dyn TurnEventSink,
        observation: &Observation,
    ) -> Result<Self::Output, PipelineError> {
        Ok(CommitOutput { result: input.query })
    }
}
