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
pub struct ExtractInput<'a> {
    pub query: &'a str,
    pub player_contribution: &'a PlayerContribution,
    pub queries: &'a [SemanticQuery],
}

#[derive(Serialize)]
pub struct ExtractOutput {
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

impl<'a> Pipeline<ExtractInput<'a>> for ExtractPipeline {
    type Output = ExtractOutput;

    fn stage(&self) -> PipelineStage {
        PipelineStage::Extract
    }

    async fn execute(
        &self,
        _input: ExtractInput<'a>,
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
            narrative: change.narrative,
            life_cycle: change.life_cycle,
        })
    }
}
