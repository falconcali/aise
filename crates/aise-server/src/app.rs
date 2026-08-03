//! Composition root: wires concrete store/LLM/runtime once, in one place
//! (R-LAYER-02).

use std::sync::Arc;

use aise::AiseEngine;
use aise::character::CharacterThinkPipeline;
use aise::context::{BaselineContextBuilder, ContextRetrievalPipeline};
use aise::llm::{LlmLimiter, LlmProvider, OpenAiCompatProvider};
use aise::persistence::{SqliteStore, Store, TurnCommitter};
use aise::planning::WriterPlanner;
use aise::runtime::{TurnInitializer, TurnRuntime};
use aise::story::StoryGenerator;
use aise::validation::ValidationPipeline;

use crate::config::ServerConfig;

/// Builds the fully wired engine. Called once at startup.
pub async fn build_engine(config: &ServerConfig) -> Result<Arc<AiseEngine>, anyhow::Error> {
    let limiter = LlmLimiter::new(config.aise.llm.max_concurrent);
    let llm: Arc<dyn LlmProvider> = Arc::new(OpenAiCompatProvider::new(config.aise.llm.clone(), limiter));

    let store: Arc<dyn Store> = SqliteStore::connect(&config.aise.storage.database_url).await?;

    let runtime = TurnRuntime::new(vec![
        Box::<TurnInitializer>::default(),
        Box::new(BaselineContextBuilder),
        Box::new(WriterPlanner),
        Box::new(ContextRetrievalPipeline),
        Box::new(CharacterThinkPipeline),
        Box::new(StoryGenerator::new(llm.clone())),
        Box::new(ValidationPipeline::default()),
        // StoryRepairer and the bounded Validation/Repair loop land with the
        // runtime budget enforcement (R-AISE-06).
        Box::new(TurnCommitter::new(store.clone())),
    ]);

    Ok(Arc::new(AiseEngine::new(runtime, store, llm, config.aise.clone())))
}
