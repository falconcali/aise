use crate::pipeline::common::{Pipeline, PipelineStage, PipelineError};
use crate::core::{TurnControl, TurnEventSink};
use crate::trace::Observation;

pub struct CommitInput {
    pub query: String
}

pub struct CommitOutput {
    pub result: String
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
        observation: &Observation
    ) -> Result<Self::Output, PipelineError> {
        Ok(CommitOutput { result: input.query })
    }
}