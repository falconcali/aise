use crate::config::ServerConfig;
use crate::trace::{NoopRedactor, TraceRedactor, TraceSinkError, TraceWriter, TraceWriterConfig};
use aise::AiseEngine;
use aise::character::CharacterThinkPipeline;
use aise::context::{BaselineContextBuilder, ContextRetrievalPipeline};
use aise::core::turn_trace::TraceSpanSink;
use aise::engine::{SystemClock, UuidIdGenerator};
use aise::llm::{LlmGateway, LlmProvider, OpenAiCompatProvider};
use aise::persistence::{SqliteStore, Store, TurnCommitter};
use aise::planning::WriterPlanner;
use aise::runtime::{StoryTurnCoordinator, TurnInitializer, TurnPipelineSet, TurnRuntime};
use aise::story::{StoryGenerator, StoryRepairer};
use aise::validation::ValidationPipeline;
use std::sync::Arc;

pub fn new_trace_writer(config: &ServerConfig) -> Result<Arc<TraceWriter>, TraceSinkError> {
    let writer_config: TraceWriterConfig = config.trace_writer_config();
    let redactor: Arc<dyn TraceRedactor> = Arc::new(NoopRedactor);
    TraceWriter::new(writer_config, config.trace_dir.clone(), redactor)
}

pub async fn build_engine(
    config: &ServerConfig,
    trace_sink: Arc<dyn TraceSpanSink>,
) -> Result<Arc<AiseEngine>, anyhow::Error> {
    let provider: Arc<dyn LlmProvider> = Arc::new(OpenAiCompatProvider::new(config.aise.llm.clone()));
    let gateway = Arc::new(LlmGateway::new(provider, config.aise.llm.clone())?);

    let store: Arc<dyn Store> = SqliteStore::connect(&config.aise.storage.database_url).await?;

    let coordinator = StoryTurnCoordinator::new(&config.aise.coordinator);

    let pipeline_set = TurnPipelineSet::builder()
        .initializer(Box::<TurnInitializer>::default())
        .baseline_builder(Box::new(BaselineContextBuilder::new(
            store.clone(),
            config.aise.content.clone(),
        )))
        .writer_planner(Box::new(WriterPlanner::new(gateway.clone())))
        .retrieval(Box::new(ContextRetrievalPipeline))
        .character_think(Box::new(CharacterThinkPipeline::new(gateway.clone())))
        .story_generator(Box::new(StoryGenerator::new(gateway.clone())))
        .validation(Box::new(ValidationPipeline::default()))
        .story_repairer(Box::new(StoryRepairer::new(gateway.clone())))
        .committer(Box::new(TurnCommitter::new(store.clone())))
        .build()?;
    let runtime = TurnRuntime::new(pipeline_set);

    let engine = AiseEngine::new(
        runtime,
        store,
        coordinator,
        config.aise.clone(),
        Arc::new(UuidIdGenerator),
        Arc::new(SystemClock),
    )
    .with_trace_sink(trace_sink);

    Ok(Arc::new(engine))
}
