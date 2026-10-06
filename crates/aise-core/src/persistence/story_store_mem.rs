use super::error::PersistenceError;
use super::story_store::StoryStore;
use crate::core::story::{StoryCommit, StoryContext, StoryInstanceInfo, StoryInstanceSpec};
use crate::core::{IdempotencyKey, StoryId};
use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

const DEFAULT_MAX_RECENT_TURNS: usize = 32;

#[derive(Debug, Clone)]
pub struct StoryStoreMemConfig {
    pub max_recent_turns: usize,
}

impl Default for StoryStoreMemConfig {
    fn default() -> Self {
        Self {
            max_recent_turns: DEFAULT_MAX_RECENT_TURNS,
        }
    }
}

#[derive(Debug)]
pub struct StoryStoreMem {
    stories: Arc<RwLock<HashMap<StoryId, StoredStory>>>,
    config: StoryStoreMemConfig,
}

#[derive(Debug, Clone)]
struct StoredStory {
    context: StoryContext,
    info: StoryInstanceInfo,
    committed: HashMap<IdempotencyKey, StoryCommit>,
}

impl StoryStoreMem {
    pub fn new() -> Self {
        Self::with_config(StoryStoreMemConfig::default())
    }

    pub fn with_config(config: StoryStoreMemConfig) -> Self {
        Self {
            stories: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    fn trim_recent_turns(context: &mut StoryContext, max_recent_turns: usize) {
        while context.rencent_turns.len() > max_recent_turns {
            context.rencent_turns.pop_front();
        }
    }
}

impl Default for StoryStoreMem {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StoryStore for StoryStoreMem {
    async fn create(
        &self,
        spec: &StoryInstanceSpec,
    ) -> Result<StoryInstanceInfo, PersistenceError> {
        let context = StoryContext {
            story_id: spec.story_id.clone(),
            pack_ref: spec.pack_ref.clone(),
            cast: spec.cast.clone(),
            player: spec.player_role.clone(),
            turn_number: crate::core::TurnNumber::new(0),
            summary: None,
            rencent_turns: std::collections::VecDeque::new(),
            life_cycle: crate::core::story::StoryLifeCycle::Active,
            world_state: crate::core::WorldState::default(),
        };
        let info = StoryInstanceInfo {
            story_id: spec.story_id.clone(),
            description: String::new(),
            created_at: spec.created_at,
            updated_at: spec.created_at,
            life_cycle: crate::core::story::StoryLifeCycle::Active,
        };
        let stored = StoredStory {
            context,
            info: info.clone(),
            committed: HashMap::new(),
        };
        let mut stories = self.stories.write().await;
        if stories.contains_key(&spec.story_id) {
            return Err(PersistenceError::ConstraintViolation {
                constraint: "story_id already exists".to_owned(),
            });
        }
        stories.insert(spec.story_id.clone(), stored);
        Ok(info)
    }

    async fn get_info(&self, story_id: &StoryId) -> Result<StoryInstanceInfo, PersistenceError> {
        let stories = self.stories.read().await;
        stories
            .get(story_id)
            .map(|story| story.info.clone())
            .ok_or(PersistenceError::NotFound)
    }

    async fn load(&self, story_id: &StoryId) -> Result<StoryContext, PersistenceError> {
        let stories = self.stories.read().await;
        stories
            .get(story_id)
            .map(|story| story.context.clone())
            .ok_or(PersistenceError::NotFound)
    }

    async fn find_committed(
        &self,
        story_id: &StoryId,
        idempotency_key: &IdempotencyKey,
    ) -> Result<Option<StoryCommit>, PersistenceError> {
        let stories = self.stories.read().await;
        let story = stories.get(story_id).ok_or(PersistenceError::NotFound)?;
        Ok(story.committed.get(idempotency_key).cloned())
    }

    async fn commit(&self, commit: &StoryCommit) -> Result<StoryCommit, PersistenceError> {
        let mut stories = self.stories.write().await;
        let story = stories
            .get_mut(&commit.story_id)
            .ok_or(PersistenceError::NotFound)?;
        let idempotency_key = commit.turn.idempotency_key.clone();
        if let Some(existing) = story.committed.get(&idempotency_key) {
            return Ok(existing.clone());
        }
        let expected_turn_number = story.context.turn_number.increment();
        if commit.turn.turn_number != expected_turn_number {
            return Err(PersistenceError::ConstraintViolation {
                constraint: "turn number does not follow the current story context".to_owned(),
            });
        }
        let mut next_context = story.context.clone();
        next_context.turn_number = commit.turn.turn_number.clone();
        next_context.rencent_turns.push_back(commit.turn.clone());
        if let crate::core::Change::Replaced(summary) = &commit.summary {
            next_context.summary = Some(summary.clone());
        }
        Self::trim_recent_turns(&mut next_context, self.config.max_recent_turns);
        story.context = next_context;
        story.info.updated_at = Utc::now();
        story.committed.insert(idempotency_key, commit.clone());
        Ok(commit.clone())
    }
}