use crate::core::{
    Change, NarrativeState, PlayerContribution, SemanticQuery, StoryContext, StoryLifeCycle, TurnControl,
};
use crate::persistence::Store;
use crate::pipeline::common::{Pipeline, PipelineError, PipelineStage};
use crate::pipeline::extract::extract_narrative;
use crate::trace::Observation;
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Serialize)]
pub struct ExtractInput {
    pub query: String,
    pub player_contribution: PlayerContribution,
    pub queries: Vec<SemanticQuery>,
}

#[derive(Serialize)]
pub struct ExtractOutput {
    pub result: String,
    pub player_contribution: PlayerContribution,
    pub narrative: Change<NarrativeState>,
    pub life_cycle: Change<StoryLifeCycle>,
}

pub struct ExtractPipeline {
    store: Arc<Store>,
}

impl ExtractPipeline {
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }
}

impl Pipeline for ExtractPipeline {
    type Input = ExtractInput;
    type Output = ExtractOutput;

    fn stage(&self) -> PipelineStage {
        PipelineStage::Extract
    }

    async fn execute(
        &self,
        input: Self::Input,
        story_ctx: &StoryContext,
        _control: &TurnControl,
        _observation: &Observation,
    ) -> Result<Self::Output, PipelineError> {
        let story_pack = self
            .store
            .pack_store()
            .get_pack(&story_ctx.pack_ref)
            .await
            .map_err(PipelineError::persistence(PipelineStage::Extract))?;
        let semantic_results = BTreeMap::new();
        let change = extract_narrative::resolve_narrative(&story_pack, story_ctx, &semantic_results);
        Ok(ExtractOutput {
            result: input.query,
            player_contribution: input.player_contribution,
            narrative: change.narrative,
            life_cycle: change.life_cycle,
        })
    }
}
