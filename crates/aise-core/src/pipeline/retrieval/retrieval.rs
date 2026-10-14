use crate::core::{EntryOrigin, FactEntry, FactId, RumorEntry, RumorId, StoryContext, StoryPack, TurnControl};
use crate::persistence::Store;
use crate::pipeline::common::{Pipeline, PipelineError, PipelineStage};
use crate::pipeline::retrieval::retrieval_trace::{self, RetrievalTraceMetrics};
use crate::trace::Observation;
use serde::Serialize;
use std::sync::Arc;

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

pub struct RetrievalPipeline {
    store: Arc<Store>,
}

impl RetrievalPipeline {
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }

    async fn load_story_pack(&self, story_ctx: &StoryContext) -> Result<Arc<StoryPack>, PipelineError> {
        self.store
            .pack_store()
            .get_pack(&story_ctx.pack_ref)
            .await
            .map_err(PipelineError::persistence(PipelineStage::Retrieval))
    }

    fn resolve(&self, story_pack: &StoryPack, input: &RetrievalInput<'_>) -> Result<RetrievalOutput, PipelineError> {
        let facts = input
            .fact_ids
            .iter()
            .map(|id| {
                story_pack
                    .seed_facts
                    .get(id)
                    .cloned()
                    .map(|body| FactEntry {
                        id: id.clone(),
                        origin: EntryOrigin::Seed,
                        body,
                    })
                    .ok_or_else(|| {
                        PipelineError::inconsistent_data(
                            PipelineStage::Retrieval,
                            "retrieval_fact_missing",
                            format!("selected fact ID {id} is missing from the story pack"),
                        )
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let rumors = input
            .rumor_ids
            .iter()
            .map(|id| {
                story_pack
                    .seed_rumors
                    .get(id)
                    .cloned()
                    .map(|body| RumorEntry {
                        id: id.clone(),
                        origin: EntryOrigin::Seed,
                        body,
                    })
                    .ok_or_else(|| {
                        PipelineError::inconsistent_data(
                            PipelineStage::Retrieval,
                            "retrieval_rumor_missing",
                            format!("selected rumor ID {id} is missing from the story pack"),
                        )
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(RetrievalOutput { facts, rumors })
    }

    async fn retrieve(
        &self,
        input: &RetrievalInput<'_>,
        story_ctx: &StoryContext,
    ) -> Result<RetrievalOutput, PipelineError> {
        let story_pack = self.load_story_pack(story_ctx).await?;
        self.resolve(story_pack.as_ref(), input)
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
        let mut metrics = RetrievalTraceMetrics {
            facts_requested: input.fact_ids.len(),
            rumors_requested: input.rumor_ids.len(),
            ..RetrievalTraceMetrics::default()
        };
        let result = self.retrieve(&input, story_ctx).await;
        if let Ok(output) = &result {
            metrics.facts_resolved = output.facts.len();
            metrics.rumors_resolved = output.rumors.len();
        }
        retrieval_trace::finish_retrieval(retrieval_observation, &result, &metrics);
        result
    }
}

#[cfg(test)]
#[path = "tests/retrieval_tests.rs"]
mod tests;
