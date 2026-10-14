use crate::core::{FactEntry, FactId, RumorEntry, RumorId, StoryContext, TurnControl};
use crate::pipeline::common::{Pipeline, PipelineError, PipelineStage};
use crate::trace::Observation;
use serde::Serialize;

#[derive(Serialize)]
pub struct RetrievalInput<'a> {
    pub fact_ids: &'a [FactId],
    pub rumor_ids: &'a [RumorId],
}

#[derive(Default, Serialize)]
pub struct RetrievalOutput {
    pub facts: Vec<FactEntry>,
    pub rumors: Vec<RumorEntry>,
}

pub struct RetrievalPipeline;

impl<'a> Pipeline<RetrievalInput<'a>> for RetrievalPipeline {
    type Output = RetrievalOutput;

    fn stage(&self) -> PipelineStage {
        PipelineStage::Retrieval
    }

    async fn execute(
        &self,
        _input: RetrievalInput<'a>,
        _story_ctx: &StoryContext,
        _control: &TurnControl,
        _observation: &Observation,
    ) -> Result<Self::Output, PipelineError> {
        Ok(RetrievalOutput::default())
    }
}
