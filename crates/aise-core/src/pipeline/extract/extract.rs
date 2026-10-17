use crate::core::{
    Change, KnowledgeChange, NarrativeConditionId, NarrativeState, SemanticQuery, StoryContext, StoryLifeCycle,
    StoryPack, Truth, TurnControl,
};
use crate::llm::LlmGateway;
use crate::persistence::Store;
use crate::pipeline::common::{Pipeline, PipelineError, PipelineStage};
use crate::pipeline::extract::{
    ExtractConfig, extract_llm, extract_narrative, extract_prompt, extract_prompt::EvaluateNarrativePrompt,
    extract_trace,
};
use crate::prompt::Prompt;
use crate::trace::Observation;
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Serialize)]
pub struct ExtractInput<'a> {
    pub proposal: &'a str,
}

#[derive(Serialize)]
pub struct ExtractOutput {
    pub knowledge_change: KnowledgeChange,
    pub narrative: Change<NarrativeState>,
    pub life_cycle: Change<StoryLifeCycle>,
}

pub struct ExtractPipeline {
    gateway: Arc<LlmGateway>,
    prompt: Arc<Prompt>,
    store: Arc<Store>,
    config: ExtractConfig,
}

impl ExtractPipeline {
    pub fn new(gateway: Arc<LlmGateway>, prompt: Arc<Prompt>, store: Arc<Store>, config: ExtractConfig) -> Self {
        Self {
            gateway,
            prompt,
            store,
            config,
        }
    }

    async fn evaluate_narrative(
        &self,
        story_pack: &StoryPack,
        story_ctx: &StoryContext,
        proposal: &str,
        queries: &[SemanticQuery],
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<BTreeMap<NarrativeConditionId, Truth>, PipelineError> {
        let rendered_prompt = extract_prompt::evaluate_narrative(
            self.prompt.as_ref(),
            EvaluateNarrativePrompt {
                story_pack,
                story_ctx,
                proposal,
                queries,
            },
            observation,
        )?;
        let raw = extract_llm::evaluate_narrative(
            self.gateway.as_ref(),
            rendered_prompt.into_messages(),
            control,
            observation,
        )
        .await
        .map_err(PipelineError::llm(PipelineStage::Extract))?;
        extract_llm::parse_narrative_evaluations(&raw, queries)
    }
}

impl<'a> Pipeline<ExtractInput<'a>> for ExtractPipeline {
    type Output = ExtractOutput;

    fn stage(&self) -> PipelineStage {
        PipelineStage::Extract
    }

    async fn execute(
        &self,
        input: ExtractInput<'a>,
        story_ctx: &StoryContext,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<Self::Output, PipelineError> {
        self.config.validate()?;
        let story_pack = self
            .store
            .pack_store()
            .get_pack(&story_ctx.pack_ref)
            .await
            .map_err(PipelineError::persistence(PipelineStage::Extract))?;
        let queries = extract_narrative::collect_semantic_queries(&story_pack, story_ctx)
            .map_err(PipelineError::narrative(PipelineStage::Extract))?;
        let semantic_results = if queries.is_empty() {
            BTreeMap::new()
        } else {
            let narrative_observation = extract_trace::begin_evaluate_narrative(observation, input.proposal, &queries);
            let result = self
                .evaluate_narrative(
                    &story_pack,
                    story_ctx,
                    input.proposal,
                    &queries,
                    control,
                    &narrative_observation,
                )
                .await;
            extract_trace::finish_evaluate_narrative(narrative_observation, &queries, &result);
            result?
        };
        let change = extract_narrative::resolve_narrative(&story_pack, story_ctx, &semantic_results);
        Ok(ExtractOutput {
            knowledge_change: KnowledgeChange::default(),
            narrative: change.narrative,
            life_cycle: change.life_cycle,
        })
    }
}

#[cfg(test)]
#[path = "tests/extract_tests.rs"]
mod tests;
