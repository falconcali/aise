use crate::core::{StoryContext, TurnControl};
use crate::pipeline::common::{Pipeline, PipelineError, PipelineStage};
use crate::trace::Observation;
use serde::Serialize;

#[derive(Serialize)]
pub struct RetrievalInput<'a> {
    pub query: &'a str,
}

#[derive(Serialize)]
pub struct RetrievalOutput {
    pub result: String,
}

pub struct RetrievalPipeline;

impl<'a> Pipeline<RetrievalInput<'a>> for RetrievalPipeline {
    type Output = RetrievalOutput;

    fn stage(&self) -> PipelineStage {
        PipelineStage::Retrieval
    }

    async fn execute(
        &self,
        input: RetrievalInput<'a>,
        story_ctx: &StoryContext,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<Self::Output, PipelineError> {
        Ok(RetrievalOutput {
            result: input.query.to_owned(),
        })
    }
}
