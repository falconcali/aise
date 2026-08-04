use crate::config::ServerConfig;
use aise::AiseEngine;
use aise::character::CharacterThinkPipeline;
use aise::context::{BaselineContextBuilder, ContextRetrievalPipeline};
use aise::llm::{LlmGateway, LlmProvider, OpenAiCompatProvider};
use aise::persistence::{SqliteStore, Store, TurnCommitter};
use aise::planning::WriterPlanner;
use aise::runtime::{StoryTurnCoordinator, TurnInitializer, TurnRuntime};
use aise::story::StoryGenerator;
use aise::validation::ValidationPipeline;
use std::sync::Arc;

pub async fn build_engine(config: &ServerConfig) -> Result<Arc<AiseEngine>, anyhow::Error> {
    let provider: Arc<dyn LlmProvider> = Arc::new(OpenAiCompatProvider::new(config.aise.llm.clone()));
    let gateway = Arc::new(LlmGateway::new(provider, config.aise.llm.clone())?);

    let store: Arc<dyn Store> = SqliteStore::connect(&config.aise.storage.database_url).await?;

    let coordinator = StoryTurnCoordinator::new(&config.aise.coordinator);

    let runtime = TurnRuntime::new(vec![
        Box::<TurnInitializer>::default(),
        Box::new(BaselineContextBuilder::new(store.clone())),
        Box::new(WriterPlanner),
        Box::new(ContextRetrievalPipeline),
        Box::new(CharacterThinkPipeline),
        Box::new(StoryGenerator::new(gateway.clone())),
        Box::new(ValidationPipeline::default()),
        Box::new(TurnCommitter::new(store.clone())),
    ]);

    Ok(Arc::new(AiseEngine::new(runtime, store, coordinator, config.aise.clone())))
}
