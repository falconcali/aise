use super::{AiseFactory, EngineConfig, EngineError, StoryCreationSpec};
use crate::core::{StoryCommit, StoryId, StoryInstanceInfo, TurnControl, TurnEvent, TurnEventSink, TurnRequest};
use crate::persistence::StoryStore;
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
        turn_control: TurnControl,
        sink: &dyn TurnEventSink,
        trace: &Trace,
    ) -> Result<StoryCommit, EngineError>;
}

pub struct AiseEngine {
    runtime: Runtime,
    story_store: Arc<dyn StoryStore>,
}

impl AiseEngine {
    pub fn new(engine_config: EngineConfig) -> Result<Self, EngineError> {
        let gateway = AiseFactory.create_llm_gateway(engine_config.llm_config);
        let prompt = AiseFactory.create_prompt(engine_config.prompt_config)?;
        let store = AiseFactory.create_story_store(engine_config.persistence_config);
        Ok(Self {
            runtime: Runtime::new(gateway, Arc::new(prompt)),
            story_store: store,
        })
    }
}

#[async_trait]
impl Engine for AiseEngine {
    async fn create_story(&self, spec: StoryCreationSpec) -> Result<StoryInstanceInfo, EngineError> {
        AiseFactory.create_story(Arc::clone(&self.story_store), spec).await
    }

    async fn remove_story(&self, story_id: &StoryId) -> Result<(), EngineError> {
        AiseFactory.remove_story(Arc::clone(&self.story_store), story_id).await
    }

    async fn get_story_info(&self, story_id: &StoryId) -> Result<StoryInstanceInfo, EngineError> {
        self.story_store.get_info(story_id).await.map_err(EngineError::from)
    }

    async fn run_turn(
        &self,
        turn_request: TurnRequest,
        turn_control: TurnControl,
        sink: &dyn TurnEventSink,
        trace: &Trace,
    ) -> Result<StoryCommit, EngineError> {
        sink.emit(TurnEvent::StageStarted {
            stage: "initialization".to_string(),
        })
        .map_err(|error| EngineError::Turn {
            message: error.to_string(),
        })?;

        let story_ctx = self
            .story_store
            .load(&turn_request.story_id)
            .await
            .map_err(|error| EngineError::Turn {
                message: error.to_string(),
            })?;

        let story_commit = self
            .runtime
            .run_turn(&story_ctx, turn_request, turn_control, sink, trace)
            .await
            .map_err(|error| EngineError::Turn {
                message: error.to_string(),
            })?;

        sink.emit(TurnEvent::Committed).map_err(|error| EngineError::Turn {
            message: error.to_string(),
        })?;

        Ok(story_commit)
    }
}
