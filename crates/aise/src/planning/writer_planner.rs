use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_data::{StoryGoal, WriterPlan};
use crate::core::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::error::AiseError;
use async_trait::async_trait;

#[derive(Default)]
pub struct WriterPlanner;

#[async_trait]
impl TurnExecutionPipeline for WriterPlanner {
    fn stage(&self) -> TurnStage {
        TurnStage::WriterPlanner
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), AiseError> {
        ctx.set_writer_plan(WriterPlan {
            retrieval_requests: Vec::new(),
            character_requests: Vec::new(),
            story_goal: StoryGoal {
                summary: ctx.player_input().to_string(),
            },
        })
    }
}
