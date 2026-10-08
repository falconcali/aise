use super::config::StoryStoreConfig;
use super::error::PersistenceError;
use super::story_store::StoryStore;
use crate::core::{
    IdempotencyKey, PackId, StoryCommit, StoryContext, StoryId, StoryInstanceInfo, StoryInstanceSpec, StoryLifeCycle,
    StoryPack, TurnNumber, WorldState,
};
use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct StoryStoreMem {
    stories: Arc<RwLock<HashMap<StoryId, StoredStory>>>,
    config: StoryStoreConfig,
}

#[derive(Debug, Clone)]
struct StoredStory {
    context: StoryContext,
    info: StoryInstanceInfo,
    committed: HashMap<IdempotencyKey, StoryCommit>,
}

impl StoryStoreMem {
    pub fn new() -> Self {
        Self::with_config(StoryStoreConfig::default())
    }

    pub fn with_config(config: StoryStoreConfig) -> Self {
        Self {
            stories: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    fn adjust_rencent_turns(context: &mut StoryContext, max_rencent_turns: usize) {
        while context.rencent_turns.len() > max_rencent_turns {
            context.rencent_turns.pop_front();
        }

        let Some(summary) = context.summary.as_ref() else {
            return;
        };

        let covered_through = summary.covered_through.value();
        while context
            .rencent_turns
            .front()
            .is_some_and(|turn| turn.turn_number.value() <= covered_through)
        {
            context.rencent_turns.pop_front();
        }
    }
}

#[cfg(test)]
#[path = "test/story_store_mem_tests.rs"]
mod tests;

impl Default for StoryStoreMem {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StoryStore for StoryStoreMem {
    async fn create(&self, spec: StoryInstanceSpec) -> Result<StoryInstanceInfo, PersistenceError> {
        let now = Utc::now();
        let context = StoryContext {
            story_id: spec.story_id,
            pack_ref: spec.pack_ref,
            cast: spec.cast,
            player: spec.player_role,
            turn_number: TurnNumber::new(0),
            summary: None,
            rencent_turns: std::collections::VecDeque::new(),
            life_cycle: StoryLifeCycle::Active,
            world_state: WorldState {},
        };
        let info = StoryInstanceInfo {
            story_id: context.story_id.clone(),
            description: String::new(),
            created_at: now,
            updated_at: now,
            life_cycle: StoryLifeCycle::Active,
        };
        let stored = StoredStory {
            context,
            info: info.clone(),
            committed: HashMap::new(),
        };
        let mut stories = self.stories.write().await;
        if stories.contains_key(&info.story_id) {
            return Err(PersistenceError::ConstraintViolation {
                constraint: "story_id already exists".to_owned(),
            });
        }
        stories.insert(info.story_id.clone(), stored);
        Ok(info)
    }

    async fn remove(&self, story_id: &StoryId) -> Result<(), PersistenceError> {
        let mut stories = self.stories.write().await;
        stories.remove(story_id).ok_or(PersistenceError::NotFound)?;
        Ok(())
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

    async fn commit(&self, commit: StoryCommit) -> Result<StoryCommit, PersistenceError> {
        let mut stories = self.stories.write().await;
        let story = stories.get_mut(&commit.story_id).ok_or(PersistenceError::NotFound)?;
        let idempotency_key = commit.turn.idempotency_key.clone();
        if let Some(existing) = story.committed.get(&idempotency_key) {
            return Ok(existing.clone());
        }
        let expected_turn_number = story.context.turn_number.increment();
        if commit.turn.turn_number.value() != expected_turn_number.value() {
            return Err(PersistenceError::ConstraintViolation {
                constraint: "turn number does not follow the current story context".to_owned(),
            });
        }
        let mut next_context = story.context.clone();
        next_context.turn_number = commit.turn.turn_number.clone();
        next_context.rencent_turns.push_back(commit.turn.clone());
        if let crate::core::Change::Replaced(summary) = &commit.summary {
            if summary.covered_through.value() > commit.turn.turn_number.value() {
                return Err(PersistenceError::ConstraintViolation {
                    constraint: "summary covers a future turn".to_owned(),
                });
            }
            if story
                .context
                .summary
                .as_ref()
                .is_some_and(|previous| summary.covered_through.value() < previous.covered_through.value())
            {
                return Err(PersistenceError::ConstraintViolation {
                    constraint: "summary coverage cannot move backwards".to_owned(),
                });
            }
            next_context.summary = Some(summary.clone());
        }
        Self::adjust_rencent_turns(&mut next_context, self.config.max_recent_turns);
        story.context = next_context;
        story.info.updated_at = Utc::now();
        story.committed.insert(idempotency_key, commit.clone());
        Ok(commit)
    }

    async fn get_pack(&self, pack_id: &PackId) -> Result<StoryPack, PersistenceError> {
        Ok(StoryPack {
            pack_id: pack_id.clone(),
            title: "白蛇传".to_string(),
            Opening: "许仙在西湖游船靠岸时，天色忽然阴沉下来。细密的雨丝打湿了青石桥面，行人纷纷避入檐下。一个白衣女子站在断桥边，没有带伞，正望着湖面出神。她回头看向许仙，礼貌地问能否借伞同行。许仙把雨伞递过去时，远处传来一声闷雷，湖心荡开一圈不合时宜的涟漪。女子自称白素贞，并说日后一定归还这把伞。".to_string(),
        })
    }
}
