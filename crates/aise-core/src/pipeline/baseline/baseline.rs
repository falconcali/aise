use crate::core::{ActivatedKnowledge, PlayerContribution, StoryContext, StoryPack, TurnControl};
use crate::llm::LlmGateway;
use crate::persistence::Store;
use crate::pipeline::baseline::baseline_knowledge::{self, KnowledgeScope};
use crate::pipeline::baseline::baseline_narrative::{self, NarrativeProjection};
use crate::pipeline::baseline::{BaselineConfig, baseline_llm, baseline_prompt, baseline_trace};
use crate::pipeline::common::{Pipeline, PipelineError, PipelineStage};
use crate::prompt::Prompt;
use crate::trace::Observation;
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
pub struct BaselineInput {
    pub player_input: String,
}

#[derive(Serialize)]
pub struct BaselineOutput {
    pub player_contribution: PlayerContribution,
    pub narrative: NarrativeProjection,
    pub knowledge: ActivatedKnowledge,
}

pub struct BaselinePipeline {
    gateway: Arc<LlmGateway>,
    prompt: Arc<Prompt>,
    store: Arc<Store>,
    config: BaselineConfig,
}

impl BaselinePipeline {
    pub fn new(gateway: Arc<LlmGateway>, prompt: Arc<Prompt>, store: Arc<Store>, config: BaselineConfig) -> Self {
        Self {
            gateway,
            prompt,
            store,
            config,
        }
    }

    async fn load_story_pack(&self, story_ctx: &StoryContext) -> Result<Arc<StoryPack>, PipelineError> {
        self.store
            .pack_store()
            .get_pack(&story_ctx.pack_ref)
            .await
            .map_err(PipelineError::persistence(PipelineStage::Baseline))
    }

    fn project_narrative(
        &self,
        story_pack: &StoryPack,
        story_ctx: &StoryContext,
        observation: &Observation,
    ) -> Result<NarrativeProjection, PipelineError> {
        baseline_narrative::project_narrative(story_pack, story_ctx, observation)
            .map_err(PipelineError::narrative(PipelineStage::Baseline))
    }

    async fn process_player_input(
        &self,
        input: &BaselineInput,
        story_ctx: &StoryContext,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<PlayerContribution, PipelineError> {
        let rendered_prompt = baseline_prompt::process_player_input(
            self.prompt.as_ref(),
            story_ctx,
            &input.player_input,
            self.store.as_ref(),
            observation,
        )
        .await?;

        let player_contribution = baseline_llm::process_player_input(
            self.gateway.as_ref(),
            &input.player_input,
            rendered_prompt.into_messages(),
            control,
            observation,
        )
        .await
        .map_err(PipelineError::llm(PipelineStage::Baseline))?;

        Ok(player_contribution)
    }

    async fn generate_player_contribution(
        &self,
        input: &BaselineInput,
        story_ctx: &StoryContext,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<PlayerContribution, PipelineError> {
        let input_observation = baseline_trace::begin_player_contribution(observation, &input.player_input);
        let result = self.process_player_input(input, story_ctx, control, &input_observation).await;
        baseline_trace::finish_player_contribution(input_observation, &result);
        result
    }

    fn activate_knowledge(
        &self,
        story_pack: &StoryPack,
        story_ctx: &StoryContext,
        player_contribution: &PlayerContribution,
        narrative: &NarrativeProjection,
        observation: &Observation,
    ) -> ActivatedKnowledge {
        baseline_knowledge::activate_knowledge(
            KnowledgeScope {
                story_pack,
                story_ctx,
                player_contribution,
                narrative,
            },
            &self.config.activation,
            observation,
        )
    }
}

impl Pipeline for BaselinePipeline {
    type Input = BaselineInput;
    type Output = BaselineOutput;

    fn stage(&self) -> PipelineStage {
        PipelineStage::Baseline
    }

    async fn execute(
        &self,
        input: Self::Input,
        story_ctx: &StoryContext,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<Self::Output, PipelineError> {
        let story_pack = self.load_story_pack(story_ctx).await?;
        let narrative = self.project_narrative(&story_pack, story_ctx, observation)?;
        let player_contribution = self
            .generate_player_contribution(&input, story_ctx, control, observation)
            .await?;
        let knowledge = self.activate_knowledge(&story_pack, story_ctx, &player_contribution, &narrative, observation);

        Ok(BaselineOutput {
            player_contribution,
            narrative,
            knowledge,
        })
    }
}
