use crate::core::{
    Change, KnowledgeChange, NarrativeConditionId, NarrativeState, RoleId, RoleMemoryChangeOp, SemanticQuery,
    StoryContext, StoryLifeCycle, StoryPack, Truth, TurnControl,
};
use crate::llm::LlmGateway;
use crate::persistence::Store;
use crate::pipeline::common::{Pipeline, PipelineError, PipelineStage};
use crate::pipeline::extract::{
    ExtractConfig,
    extract_knowledge::{self, WorldKnowledgeExtraction},
    extract_llm,
    extract_narrative::{self, NarrativeChange},
    extract_prompt::{self, EvaluateNarrativePrompt, ExtractRoleMemoryPrompt, ExtractWorldKnowledgePrompt},
    extract_trace,
};
use crate::prompt::Prompt;
use crate::trace::Observation;
use futures::{StreamExt, TryStreamExt, stream};
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
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<NarrativeChange, PipelineError> {
        let queries = extract_narrative::collect_semantic_queries(story_pack, story_ctx)
            .map_err(PipelineError::narrative(PipelineStage::Extract))?;
        let semantic_results = if queries.is_empty() {
            BTreeMap::new()
        } else {
            let narrative_observation = extract_trace::begin_evaluate_narrative(observation, proposal, &queries);
            let result = self
                .evaluate_narrative_queries(story_ctx, proposal, &queries, control, &narrative_observation)
                .await;
            extract_trace::finish_evaluate_narrative(narrative_observation, &queries, &result);
            result?
        };
        Ok(extract_narrative::resolve_narrative(story_pack, story_ctx, &semantic_results))
    }

    async fn evaluate_narrative_queries(
        &self,
        story_ctx: &StoryContext,
        proposal: &str,
        queries: &[SemanticQuery],
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<BTreeMap<NarrativeConditionId, Truth>, PipelineError> {
        let rendered_prompt = extract_prompt::evaluate_narrative(
            self.prompt.as_ref(),
            EvaluateNarrativePrompt {
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

    async fn extract_world_knowledge(
        &self,
        story_ctx: &StoryContext,
        proposal: &str,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<WorldKnowledgeExtraction, PipelineError> {
        let world_observation = extract_trace::begin_extract_world_knowledge(observation, proposal);
        let result = self
            .extract_world_knowledge_inner(story_ctx, proposal, control, &world_observation)
            .await;
        extract_trace::finish_extract_world_knowledge(world_observation, &result);
        result
    }

    async fn extract_world_knowledge_inner(
        &self,
        story_ctx: &StoryContext,
        proposal: &str,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<WorldKnowledgeExtraction, PipelineError> {
        let rendered_prompt = extract_prompt::extract_world_knowledge(
            self.prompt.as_ref(),
            ExtractWorldKnowledgePrompt {
                story_ctx,
                proposal,
                config: &self.config,
            },
            observation,
        )?;
        let raw = extract_llm::extract_world_knowledge(
            self.gateway.as_ref(),
            rendered_prompt.into_messages(),
            control,
            observation,
        )
        .await
        .map_err(PipelineError::llm(PipelineStage::Extract))?;
        extract_llm::parse_world_knowledge(&raw, story_ctx, &self.config)
    }

    async fn extract_role_memories(
        &self,
        story_ctx: &StoryContext,
        proposal: &str,
        roles_for_memory: &[RoleId],
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<Vec<Vec<RoleMemoryChangeOp>>, PipelineError> {
        if roles_for_memory.is_empty() {
            return Ok(Vec::new());
        }
        stream::iter(roles_for_memory.iter().cloned())
            .map(|role_id| async move {
                self.extract_role_memory(story_ctx, proposal, &role_id, control, observation)
                    .await
            })
            .buffered(self.config.max_memory_concurrency.get())
            .try_collect()
            .await
    }

    async fn extract_role_memory(
        &self,
        story_ctx: &StoryContext,
        proposal: &str,
        role_id: &RoleId,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<Vec<RoleMemoryChangeOp>, PipelineError> {
        let role_observation = extract_trace::begin_extract_role_memory(observation, role_id, proposal);
        let result = self
            .extract_role_memory_inner(story_ctx, proposal, role_id, control, &role_observation)
            .await;
        extract_trace::finish_extract_role_memory(role_observation, &result);
        result
    }

    async fn extract_role_memory_inner(
        &self,
        story_ctx: &StoryContext,
        proposal: &str,
        role_id: &RoleId,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<Vec<RoleMemoryChangeOp>, PipelineError> {
        let rendered_prompt = extract_prompt::extract_role_memory(
            self.prompt.as_ref(),
            ExtractRoleMemoryPrompt {
                story_ctx,
                proposal,
                role_id,
                config: &self.config,
            },
            observation,
        )?;
        let raw = extract_llm::extract_role_memory(
            self.gateway.as_ref(),
            rendered_prompt.into_messages(),
            control,
            observation,
        )
        .await
        .map_err(PipelineError::llm(PipelineStage::Extract))?;
        extract_llm::parse_role_memory(&raw, story_ctx, role_id, &self.config)
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
        let (narrative_change, world_extraction) = tokio::try_join!(
            self.evaluate_narrative(&story_pack, story_ctx, input.proposal, control, observation),
            self.extract_world_knowledge(story_ctx, input.proposal, control, observation),
        )?;
        let role_memories = self
            .extract_role_memories(
                story_ctx,
                input.proposal,
                &world_extraction.roles_for_memory,
                control,
                observation,
            )
            .await?;
        let knowledge_change = extract_knowledge::assemble_knowledge_change(world_extraction, role_memories);
        Ok(ExtractOutput {
            knowledge_change,
            narrative: narrative_change.narrative,
            life_cycle: narrative_change.life_cycle,
        })
    }
}

#[cfg(test)]
#[path = "tests/extract_tests.rs"]
mod tests;
