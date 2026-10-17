use crate::core::{FactEntry, FactId, RumorEntry, RumorId, StoryContext, TurnControl};
use crate::pipeline::common::{Pipeline, PipelineError, PipelineStage};
use crate::pipeline::retrieval::retrieval_trace;
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

impl RetrievalPipeline {
    pub fn new() -> Self {
        Self
    }

    fn resolve_facts(&self, story_ctx: &StoryContext, fact_ids: &[FactId]) -> Result<Vec<FactEntry>, PipelineError> {
        fact_ids
            .iter()
            .map(|id| {
                story_ctx.facts.get(id).cloned().ok_or_else(|| {
                    PipelineError::inconsistent_data(
                        PipelineStage::Retrieval,
                        "retrieval_fact_missing",
                        format!("selected fact ID {id} is missing from the story context"),
                    )
                })
            })
            .collect()
    }

    fn resolve_rumors(
        &self,
        story_ctx: &StoryContext,
        rumor_ids: &[RumorId],
    ) -> Result<Vec<RumorEntry>, PipelineError> {
        rumor_ids
            .iter()
            .map(|id| {
                story_ctx.rumors.get(id).cloned().ok_or_else(|| {
                    PipelineError::inconsistent_data(
                        PipelineStage::Retrieval,
                        "retrieval_rumor_missing",
                        format!("selected rumor ID {id} is missing from the story context"),
                    )
                })
            })
            .collect()
    }

    fn resolve(&self, story_ctx: &StoryContext, input: &RetrievalInput<'_>) -> Result<RetrievalOutput, PipelineError> {
        Ok(RetrievalOutput {
            facts: self.resolve_facts(story_ctx, input.fact_ids)?,
            rumors: self.resolve_rumors(story_ctx, input.rumor_ids)?,
        })
    }

    fn retrieve(&self, input: &RetrievalInput<'_>, story_ctx: &StoryContext) -> Result<RetrievalOutput, PipelineError> {
        self.resolve(story_ctx, input)
    }
}

impl Default for RetrievalPipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Pipeline<RetrievalInput<'a>> for RetrievalPipeline {
    type Output = RetrievalOutput;

    fn stage(&self) -> PipelineStage {
        PipelineStage::Retrieval
    }

    async fn execute(
        &self,
        input: RetrievalInput<'a>,
        story_ctx: &StoryContext,
        _control: &TurnControl,
        observation: &Observation,
    ) -> Result<Self::Output, PipelineError> {
        let retrieval_observation = retrieval_trace::begin_retrieval(observation, &input);
        let result = self.retrieve(&input, story_ctx);
        retrieval_trace::finish_retrieval(retrieval_observation, &result);
        result
    }
}

#[cfg(test)]
#[path = "tests/retrieval_tests.rs"]
mod tests;
