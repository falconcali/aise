use super::config::StoryStoreConfig;
use super::error::PersistenceError;
use super::story_store::StoryStore;
use crate::core::{
    Change, EntryOrigin, FactChangeOp, FactEntry, FactId, IdempotencyKey, KnowledgeChange, MemoryEntry, MemoryId,
    RoleId, RoleMemoryChangeOp, RumorChangeOp, RumorEntry, RumorId, StoryCommit, StoryContext, StoryId,
    StoryInstanceInfo, StoryInstanceSpec, StoryLifeCycle, TurnNumber,
};
use async_trait::async_trait;
use chrono::Utc;
use std::collections::{HashMap, HashSet};
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
        while context.recent_turns.len() > max_rencent_turns {
            context.recent_turns.pop_front();
        }

        let Some(summary) = context.summary.as_ref() else {
            return;
        };

        let covered_through = summary.covered_through.value();
        while context
            .recent_turns
            .front()
            .is_some_and(|turn| turn.turn_number.value() <= covered_through)
        {
            context.recent_turns.pop_front();
        }
    }

    fn validate_knowledge_change(context: &StoryContext, change: &KnowledgeChange) -> Result<(), PersistenceError> {
        let mut seen_facts = HashSet::<&FactId>::with_capacity(change.facts.len());
        for op in &change.facts {
            match op {
                FactChangeOp::Add { id, .. } => {
                    if !seen_facts.insert(id) {
                        return Err(constraint_violation(format!("duplicate fact_id in knowledge_change: {id}")));
                    }
                    if context.facts.contains_key(id) {
                        return Err(constraint_violation(format!("cannot add existing fact_id: {id}")));
                    }
                }
                FactChangeOp::Update { id, .. } => {
                    if !seen_facts.insert(id) {
                        return Err(constraint_violation(format!("duplicate fact_id in knowledge_change: {id}")));
                    }
                    if !context.facts.contains_key(id) {
                        return Err(constraint_violation(format!("cannot update missing fact_id: {id}")));
                    }
                }
            }
        }

        let mut seen_rumors = HashSet::<&RumorId>::with_capacity(change.rumors.len());
        for op in &change.rumors {
            match op {
                RumorChangeOp::Add { id, .. } => {
                    if !seen_rumors.insert(id) {
                        return Err(constraint_violation(format!("duplicate rumor_id in knowledge_change: {id}")));
                    }
                    if context.rumors.contains_key(id) {
                        return Err(constraint_violation(format!("cannot add existing rumor_id: {id}")));
                    }
                }
                RumorChangeOp::Update { id, .. } => {
                    if !seen_rumors.insert(id) {
                        return Err(constraint_violation(format!("duplicate rumor_id in knowledge_change: {id}")));
                    }
                    if !context.rumors.contains_key(id) {
                        return Err(constraint_violation(format!("cannot update missing rumor_id: {id}")));
                    }
                }
                RumorChangeOp::Remove { id } => {
                    if !seen_rumors.insert(id) {
                        return Err(constraint_violation(format!("duplicate rumor_id in knowledge_change: {id}")));
                    }
                    if !context.rumors.contains_key(id) {
                        return Err(constraint_violation(format!("cannot remove missing rumor_id: {id}")));
                    }
                }
            }
        }

        let mut seen_memories = HashSet::<(&RoleId, &MemoryId)>::with_capacity(change.memories.len());
        for op in &change.memories {
            match op {
                RoleMemoryChangeOp::Add { role_id, id, .. } => {
                    if !seen_memories.insert((role_id, id)) {
                        return Err(constraint_violation(format!(
                            "duplicate memory_id {id} for role {role_id} in knowledge_change"
                        )));
                    }
                    let role = context.roles.get(role_id).ok_or_else(|| {
                        constraint_violation(format!("unknown role_id in knowledge_change: {role_id}"))
                    })?;
                    if role.memories.contains_key(id) {
                        return Err(constraint_violation(format!(
                            "cannot add existing memory_id {id} for role {role_id}"
                        )));
                    }
                }
                RoleMemoryChangeOp::Update { role_id, id, .. } => {
                    if !seen_memories.insert((role_id, id)) {
                        return Err(constraint_violation(format!(
                            "duplicate memory_id {id} for role {role_id} in knowledge_change"
                        )));
                    }
                    let role = context.roles.get(role_id).ok_or_else(|| {
                        constraint_violation(format!("unknown role_id in knowledge_change: {role_id}"))
                    })?;
                    if !role.memories.contains_key(id) {
                        return Err(constraint_violation(format!(
                            "cannot update missing memory_id {id} for role {role_id}"
                        )));
                    }
                }
            }
        }

        Ok(())
    }

    fn apply_knowledge_change(context: &mut StoryContext, turn_number: &TurnNumber, change: &KnowledgeChange) {
        for op in &change.facts {
            match op {
                FactChangeOp::Add { id, body } => {
                    context.facts.insert(
                        id.clone(),
                        FactEntry {
                            id: id.clone(),
                            origin: EntryOrigin::Turn(turn_number.clone()),
                            body: body.clone(),
                        },
                    );
                }
                FactChangeOp::Update { id, body } => {
                    if let Some(entry) = context.facts.get_mut(id) {
                        entry.body = body.clone();
                    }
                }
            }
        }

        for op in &change.rumors {
            match op {
                RumorChangeOp::Add { id, body } => {
                    context.rumors.insert(
                        id.clone(),
                        RumorEntry {
                            id: id.clone(),
                            origin: EntryOrigin::Turn(turn_number.clone()),
                            body: body.clone(),
                        },
                    );
                }
                RumorChangeOp::Update { id, body } => {
                    if let Some(entry) = context.rumors.get_mut(id) {
                        entry.body = body.clone();
                    }
                }
                RumorChangeOp::Remove { id } => {
                    context.rumors.remove(id);
                }
            }
        }

        for op in &change.memories {
            match op {
                RoleMemoryChangeOp::Add { role_id, id, body } => {
                    if let Some(role) = context.roles.get_mut(role_id) {
                        role.memories.insert(
                            id.clone(),
                            MemoryEntry {
                                id: id.clone(),
                                origin: EntryOrigin::Turn(turn_number.clone()),
                                body: body.clone(),
                            },
                        );
                    }
                }
                RoleMemoryChangeOp::Update { role_id, id, body } => {
                    if let Some(role) = context.roles.get_mut(role_id)
                        && let Some(entry) = role.memories.get_mut(id)
                    {
                        entry.body = body.clone();
                    }
                }
            }
        }
    }
}

fn constraint_violation(constraint: impl Into<String>) -> PersistenceError {
    PersistenceError::ConstraintViolation {
        constraint: constraint.into(),
    }
}

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
            roles: spec.roles,
            facts: spec.facts,
            rumors: spec.rumors,
            player: spec.player_role,
            turn_number: TurnNumber::new(0),
            summary: None,
            recent_turns: std::collections::VecDeque::new(),
            life_cycle: StoryLifeCycle::Active,
            narrative: spec.narrative,
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
        if story.context.life_cycle != StoryLifeCycle::Active {
            return Err(PersistenceError::ConstraintViolation {
                constraint: "story is not active".to_owned(),
            });
        }
        let expected_turn_number = story.context.turn_number.increment();
        if commit.turn.turn_number.value() != expected_turn_number.value() {
            return Err(PersistenceError::ConstraintViolation {
                constraint: "turn number does not follow the current story context".to_owned(),
            });
        }
        if let Change::Replaced(summary) = &commit.summary {
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
        }
        Self::validate_knowledge_change(&story.context, &commit.turn.knowledge_change)?;

        let mut next_context = story.context.clone();
        next_context.turn_number = commit.turn.turn_number.clone();
        Self::apply_knowledge_change(&mut next_context, &commit.turn.turn_number, &commit.turn.knowledge_change);
        next_context.recent_turns.push_back(commit.turn.clone());
        if let Change::Replaced(summary) = &commit.summary {
            next_context.summary = Some(summary.clone());
        }
        if let Change::Replaced(narrative) = &commit.narrative {
            next_context.narrative = narrative.clone();
        }
        if let Change::Replaced(life_cycle) = commit.life_cycle {
            next_context.life_cycle = life_cycle;
            story.info.life_cycle = life_cycle;
        }
        Self::adjust_rencent_turns(&mut next_context, self.config.max_recent_turns);
        story.context = next_context;
        story.info.updated_at = Utc::now();
        story.committed.insert(idempotency_key, commit.clone());
        Ok(commit)
    }
}

#[cfg(test)]
#[path = "tests/story_store_mem_tests.rs"]
mod tests;
