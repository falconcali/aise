use crate::domain::narrative::StoryTurn;
use crate::error::AiseError;
use crate::persistence::store::{Store, TurnCommit};
use crate::runtime::pipeline::TurnExecutionPipeline;
use crate::runtime::turn_execution_ctx::TurnExecutionContext;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct TurnCommitter {
    store: Arc<dyn Store>,
}

impl TurnCommitter {
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl TurnExecutionPipeline for TurnCommitter {
    fn stage(&self) -> &'static str {
        "turn_committer"
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), AiseError> {
        let draft = ctx
            .draft
            .as_ref()
            .ok_or_else(|| AiseError::Internal("no draft to commit".into()))?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let commit = TurnCommit {
            story_id: ctx.story_id.clone(),
            turn: StoryTurn {
                id: ctx.turn_id.clone(),
                player_input: ctx.player_input.clone(),
                story_text: draft.story_text.clone(),
                summary_delta: None,
                created_at: now,
            },
            events: draft.events.clone(),
            characters: Vec::new(),
            world: None,
            memory: Vec::new(),
            summary: String::new(),
        };
        self.store.commit_turn(&commit).await
    }
}
