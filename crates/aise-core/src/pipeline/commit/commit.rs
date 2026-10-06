use crate::core::{
    Change, IdempotencyKey, PlayerContribution, StoryCommit, StoryContext, Turn, TurnControl, TurnEvaluation,
    TurnEventSink, TurnSegment, TurnStatus, WorldChange,
};
use crate::pipeline::common::{Pipeline, PipelineError, PipelineStage};
use crate::trace::Observation;
use serde::Serialize;

#[derive(Serialize)]
pub struct CommitInput {
    pub query: String,
}

pub struct CommitPipeline;

impl Pipeline for CommitPipeline {
    type Input = CommitInput;
    type Output = StoryCommit;

    fn stage(&self) -> PipelineStage {
        PipelineStage::Commit
    }

    async fn execute(
        &self,
        story_ctx: &StoryContext,
        input: Self::Input,
        control: &TurnControl,
        sink: &dyn TurnEventSink,
        observation: &Observation,
    ) -> Result<Self::Output, PipelineError> {
        Ok(StoryCommit {
            story_id: story_ctx.story_id.clone(),
            turn: Turn {
                turn_number: story_ctx.turn_number.clone(),
                idempotency_key: IdempotencyKey::try_new("temporary-key")
                    .map_err(|error| PipelineError::new(PipelineStage::Commit, error.to_string()))?,
                player_contribution: PlayerContribution {
                    raw: String::new(),
                    processed: String::new(),
                },
                turn_segment: TurnSegment::new(input.query),
                world_change: WorldChange {},
                turn_evaluation: TurnEvaluation {},
                turn_status: TurnStatus::Accepted,
            },
            summary: Change::Unchanged,
        })
    }
}
