use super::{EngineError, StoryCreationSpec, StoryFactory};
use crate::core::{StoryId, StoryInstanceInfo, TurnControl, TurnEvent, TurnEventSink, TurnRequest, TurnResult};
use crate::llm::LlmGateway;
use crate::persistence::StoryStore;
use crate::pipeline::Runtime;
use crate::prompt::Prompt;
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
    ) -> Result<TurnResult, EngineError>;
}

pub struct AiseEngine {
    runtime: Runtime,
    factory: StoryFactory,
    story_store: Arc<dyn StoryStore>,
}

impl AiseEngine {
    pub fn new(gateway: Arc<LlmGateway>, prompt: Arc<Prompt>, store: Arc<dyn StoryStore>) -> Self {
        Self {
            runtime: Runtime::new(gateway, prompt),
            factory: StoryFactory::new(Arc::clone(&store)),
            story_store: Arc::clone(&store),
        }
    }
}

#[async_trait]
impl Engine for AiseEngine {
    async fn create_story(&self, spec: StoryCreationSpec) -> Result<StoryInstanceInfo, EngineError> {
        self.factory.create_story(spec).await
    }

    async fn remove_story(&self, story_id: &StoryId) -> Result<(), EngineError> {
        self.factory.remove_story(story_id).await
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
    ) -> Result<TurnResult, EngineError> {
        sink.emit(TurnEvent::StageStarted {
            stage: "initialization".to_string(),
        })
        .map_err(|error| EngineError::Turn {
            message: error.to_string(),
        })?;

        let turn_result = self
            .runtime
            .run_turn(turn_request, turn_control, sink, trace)
            .await
            .map_err(|error| EngineError::Turn {
                message: error.to_string(),
            })?;

        sink.emit(TurnEvent::Committed {
            result: turn_result.result.clone(),
            replayed: turn_result.replayed,
        })
        .map_err(|error| EngineError::Turn {
            message: error.to_string(),
        })?;

        Ok(turn_result)
    }
}
