use crate::context::ctx_model::{BaselineContext, StoryConfig};
use crate::error::AiseError;
use crate::persistence::store::Store;
use crate::runtime::pipeline::TurnExecutionPipeline;
use crate::runtime::turn_execution_ctx::TurnExecutionContext;
use async_trait::async_trait;
use std::sync::Arc;

pub struct BaselineContextBuilder {
    store: Arc<dyn Store>,
}

impl BaselineContextBuilder {
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl TurnExecutionPipeline for BaselineContextBuilder {
    fn stage(&self) -> &'static str {
        "baseline_ctx_builder"
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), AiseError> {
        let characters = self.store.load_characters(&ctx.story_id).await?;
        let recent_story = self.store.load_story(&ctx.story_id, ctx.budget.max_retrieved_items).await?;
        ctx.baseline_ctx = BaselineContext {
            story_instructions: String::new(),
            story_config: StoryConfig::default(),
            player_character: characters.first().cloned(),
            current_scene: None,
            relevant_characters: characters,
            recent_story: recent_story.iter().map(|t| t.story_text.clone()).collect(),
            story_summary: String::new(),
            active_constraints: Vec::new(),
        };
        Ok(())
    }
}
