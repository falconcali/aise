use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::context::ctx_model::ContextSource;
use crate::domain::ids::CharacterId;
use crate::error::AiseError;
use crate::runtime::pipeline::TurnExecutionPipeline;
use crate::runtime::turn_execution_ctx::TurnExecutionContext;

/// Planner output: what this Turn needs (Architecture.md §8).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WriterPlan {
    pub need_retrieval: bool,
    pub need_character_thinking: bool,
    pub retrieval_requests: Vec<ContextRequest>,
    pub character_requests: Vec<CharacterId>,
    pub story_goal: StoryGoal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextRequest {
    pub query: String,
    pub sources: Vec<ContextSource>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoryGoal {
    pub summary: String,
}

/// Understands the player input, decides the story goal and which context
/// gaps to fill (Architecture.md §8). Outputs `ctx.plan`.
#[derive(Default)]
pub struct WriterPlanner;

#[async_trait]
impl TurnExecutionPipeline for WriterPlanner {
    fn stage(&self) -> &'static str {
        "writer_planner"
    }

    async fn execute(&self, _ctx: &mut TurnExecutionContext) -> Result<(), AiseError> {
        // Framework stub: set ctx.plan (need_retrieval, need_character_thinking, ...).
        Ok(())
    }
}
