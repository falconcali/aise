use super::{AiseFactory, EngineConfig, EngineError, StoryCreationSpec};
use crate::core::{
    StoryCommit, StoryId, StoryInstanceInfo, StoryLifeCycle, TurnControl, TurnEvent, TurnEventSink, TurnRequest,
};
use crate::persistence::Store;
use crate::pipeline::Runtime;
use crate::trace::Trace;
use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait Engine: Send + Sync {
    async fn create_story(&self, spec: StoryCreationSpec) -> Result<StoryInstanceInfo, EngineError>;
    async fn remove_story(&self, story_id: &StoryId) -> Result<(), EngineError>;
    async fn get_story_info(&self, story_id: &StoryId) -> Result<StoryInstanceInfo, EngineError>;

    async fn run_turn(
        &self,
        turn_request: TurnRequest,
        turn_control: &TurnControl,
        sink: &dyn TurnEventSink,
        trace: &Trace,
    ) -> Result<StoryCommit, EngineError>;
}

pub struct AiseEngine {
    runtime: Runtime,
    store: Arc<Store>,
}

impl AiseEngine {
    pub fn new(engine_config: EngineConfig) -> Result<Self, EngineError> {
        let gateway = AiseFactory.create_llm_gateway(engine_config.llm_config);
        let prompt = AiseFactory.create_prompt(engine_config.prompt_config)?;
        let store = AiseFactory.create_store(engine_config.persistence_config);
        Ok(Self {
            runtime: Runtime::new(gateway, Arc::new(prompt), Arc::clone(&store), engine_config.pipeline_config)?,
            store,
        })
    }
}

#[async_trait]
impl Engine for AiseEngine {
    async fn create_story(&self, spec: StoryCreationSpec) -> Result<StoryInstanceInfo, EngineError> {
        AiseFactory.create_story(Arc::clone(&self.store), spec).await
    }

    async fn remove_story(&self, story_id: &StoryId) -> Result<(), EngineError> {
        AiseFactory.remove_story(Arc::clone(&self.store), story_id).await
    }

    async fn get_story_info(&self, story_id: &StoryId) -> Result<StoryInstanceInfo, EngineError> {
        self.store.story_store().get_info(story_id).await.map_err(EngineError::from)
    }

    async fn run_turn(
        &self,
        turn_request: TurnRequest,
        turn_control: &TurnControl,
        sink: &dyn TurnEventSink,
        trace: &Trace,
    ) -> Result<StoryCommit, EngineError> {
        sink.emit(TurnEvent::TurnStarted);

        let story_ctx = self.store.story_store().load(&turn_request.story_id).await?;
        if story_ctx.life_cycle != StoryLifeCycle::Active {
            return Err(EngineError::StoryNotActive {
                life_cycle: story_ctx.life_cycle,
            });
        }

        let story_commit = self
            .runtime
            .run_turn(&story_ctx, turn_request, turn_control, sink, trace)
            .await?;

        sink.emit(TurnEvent::TurnCommitted);
        Ok(story_commit)
    }
}
