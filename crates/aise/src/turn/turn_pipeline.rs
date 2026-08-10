use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_error::TurnExecutionError;
use async_trait::async_trait;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnStage {
    TurnInitializer,
    BaselineBuilder,
    WriterPlanner,
    Context,
    ContextRetrieval,
    CharacterThink,
    StoryGenerator,
    Validation,
    StoryRepairer,
    TurnCommitter,
}

impl TurnStage {
    pub fn as_str(&self) -> &'static str {
        match self {
            TurnStage::TurnInitializer => "turn_initializer",
            TurnStage::BaselineBuilder => "baseline_ctx_builder",
            TurnStage::WriterPlanner => "writer_planner",
            TurnStage::Context => "context",
            TurnStage::ContextRetrieval => "context_retrieval",
            TurnStage::CharacterThink => "character_think",
            TurnStage::StoryGenerator => "story_generator",
            TurnStage::Validation => "validation",
            TurnStage::StoryRepairer => "story_repairer",
            TurnStage::TurnCommitter => "turn_committer",
        }
    }
}

impl fmt::Display for TurnStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[async_trait]
pub trait TurnExecutionPipeline: Send + Sync {
    fn stage(&self) -> TurnStage;

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), TurnExecutionError>;
}
