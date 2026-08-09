use crate::config::ServerConfig;
use crate::trace::{NoopRedactor, TraceRedactor, TraceSinkError, TraceWriter, TraceWriterConfig};
use aise::AiseEngine;
use aise::character::CharacterThinkPipeline;
use aise::context::{
    BaselineContextBuilder, ContextRetrievalPipeline, EntityCandidateRetriever, TopicCandidateRetriever,
};
use aise::core::turn_trace::TraceSpanSink;
use aise::engine::{SystemClock, UuidIdGenerator};
use aise::llm::{LlmGateway, LlmProvider, OpenAiCompatProvider};
use aise::persistence::asset_store::AssetStore;
use aise::persistence::knowledge_read_port::KnowledgeReadPort;
use aise::persistence::sqlite_asset_store::SqliteAssetStore;
use aise::persistence::{SqliteStore, Store, TurnCommitter};
use aise::planning::WriterPlanner;
use aise::prompt::{CatalogPromptSource, TrustedPromptSource};
use aise::runtime::{StoryTurnCoordinator, TurnInitializer, TurnPipelineSet, TurnRuntime};
use aise::story::instance_factory::{StoryInstanceFactory, StoryInstantiationLimits};
use aise::story::pack_service::{NativeAssetImporter, PackService};
use aise::story::{StoryGenerator, StoryRepairer};
use aise::validation::ValidationPipeline;
use std::sync::Arc;

pub fn new_trace_writer(config: &ServerConfig) -> Result<Arc<TraceWriter>, TraceSinkError> {
    let writer_config: TraceWriterConfig = config.trace_writer_config();
    let redactor: Arc<dyn TraceRedactor> = Arc::new(NoopRedactor);
    TraceWriter::new(writer_config, config.trace_dir.clone(), redactor)
}

pub struct EngineServices {
    pub engine: Arc<AiseEngine>,
    pub pack_service: Arc<PackService>,
    pub instance_factory: Arc<StoryInstanceFactory>,
}

pub async fn build_services(
    config: &ServerConfig,
    trace_sink: Arc<dyn TraceSpanSink>,
) -> Result<EngineServices, anyhow::Error> {
    let provider: Arc<dyn LlmProvider> = Arc::new(OpenAiCompatProvider::new(config.aise.llm.clone()));
    let prompt_source: Arc<dyn TrustedPromptSource> = Arc::new(
        CatalogPromptSource::from_config(&config.aise.prompt)
            .map_err(|error| anyhow::anyhow!("trusted prompt source failed: {error}"))?,
    );
    let gateway = Arc::new(LlmGateway::new(provider, prompt_source, config.aise.llm.clone())?);

    let sqlite = SqliteStore::connect(&config.aise.storage.database_url).await?;
    let store: Arc<dyn Store> = sqlite.clone();
    let knowledge: Arc<dyn KnowledgeReadPort> = sqlite;

    let coordinator = StoryTurnCoordinator::new(&config.aise.coordinator);

    let retrieval = ContextRetrievalPipeline::new(
        config.aise.retrieval.clone(),
        vec![
            Arc::new(EntityCandidateRetriever::new(knowledge.clone())),
            Arc::new(TopicCandidateRetriever::new(knowledge)),
        ],
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))?;

    let pipeline_set = TurnPipelineSet::builder()
        .initializer(Box::<TurnInitializer>::default())
        .baseline_builder(Box::new(BaselineContextBuilder::new(
            store.clone(),
            config.aise.content.clone(),
            config.aise.context.clone(),
            config.aise.assets.clone(),
        )))
        .writer_planner(Box::new(WriterPlanner::new(
            gateway.clone(),
            config.aise.planner.clone(),
            config.aise.retrieval.clone(),
            config.aise.assets.clone(),
        )))
        .retrieval(Box::new(retrieval))
        .character_think(Box::new(CharacterThinkPipeline::new(
            gateway.clone(),
            config.aise.content.max_character_thought_bytes,
        )))
        .story_generator(Box::new(StoryGenerator::new(gateway.clone())))
        .validation(Box::new(ValidationPipeline::default()))
        .story_repairer(Box::new(StoryRepairer::new(gateway.clone())))
        .committer(Box::new(TurnCommitter::new(store.clone())))
        .build()?;
    let runtime = TurnRuntime::new(pipeline_set);

    let engine = AiseEngine::new(
        runtime,
        store.clone(),
        coordinator,
        config.aise.clone(),
        Arc::new(UuidIdGenerator),
        Arc::new(SystemClock),
    )
    .with_trace_sink(trace_sink);

    let asset_store: Arc<dyn AssetStore> = SqliteAssetStore::connect(&config.aise.storage.database_url)
        .await
        .map_err(|error| anyhow::anyhow!("asset store connect failed: {error}"))?;
    let importer = NativeAssetImporter::new(config.aise.assets.clone());
    let pack_service = Arc::new(PackService::new(importer, asset_store.clone()));
    let instance_factory = Arc::new(StoryInstanceFactory::new(
        asset_store,
        store,
        StoryInstantiationLimits {
            max_roles: config.aise.assets.max_roles,
            max_facts: config.aise.assets.max_world_facts,
            max_rumors: config.aise.assets.max_world_rumors,
            max_memories: config.aise.assets.max_seed_memories_per_role,
            max_relationships: config.aise.assets.max_relationships_per_role,
        },
    ));

    Ok(EngineServices {
        engine: Arc::new(engine),
        pack_service,
        instance_factory,
    })
}

pub async fn build_engine(
    config: &ServerConfig,
    trace_sink: Arc<dyn TraceSpanSink>,
) -> Result<Arc<AiseEngine>, anyhow::Error> {
    Ok(build_services(config, trace_sink).await?.engine)
}
