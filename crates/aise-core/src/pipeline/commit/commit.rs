use crate::core::{
    Change, IdempotencyKey, PlayerContribution, StoryCommit, StoryContext, StorySummary, Turn, TurnControl,
    TurnEvaluation, TurnEventSink, TurnNumber, TurnSegment, TurnStatus, WorldChange,
};
use crate::persistence::StoryStore;
use crate::pipeline::commit::commit_trace;
use crate::pipeline::common::{Pipeline, PipelineError, PipelineStage};
use crate::trace::Observation;
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
pub struct CommitInput {
    pub player_contribution: PlayerContribution,
    pub idempotency_key: IdempotencyKey,
    pub turn_segment: TurnSegment,
    pub world_change: WorldChange,
    pub turn_evaluation: TurnEvaluation,
    pub summary: Change<StorySummary>,
}

pub struct CommitPipeline {
    story_store: Arc<dyn StoryStore>,
}

impl CommitPipeline {
    pub fn new(story_store: Arc<dyn StoryStore>) -> Self {
        Self { story_store }
    }
}

impl Pipeline for CommitPipeline {
    type Input = CommitInput;
    type Output = StoryCommit;

    fn stage(&self) -> PipelineStage {
        PipelineStage::Commit
    }

    async fn execute(
        &self,
        input: Self::Input,
        story_ctx: &StoryContext,
        control: &TurnControl,
        sink: &dyn TurnEventSink,
        observation: &Observation,
    ) -> Result<Self::Output, PipelineError> {
        let story_commit = StoryCommit {
            story_id: story_ctx.story_id.clone(),
            turn: Turn {
                turn_number: TurnNumber::new(story_ctx.turn_number.value() + 1),
                idempotency_key: input.idempotency_key,
                player_contribution: input.player_contribution,
                turn_segment: input.turn_segment,
                world_change: input.world_change,
                turn_evaluation: input.turn_evaluation,
                turn_status: TurnStatus::Accepted,
            },
            summary: input.summary,
        };

        let commit_observation = commit_trace::begin_commit(observation, &story_commit);
        let result = self
            .story_store
            .commit(story_commit)
            .await
            .map_err(|error| PipelineError::new(PipelineStage::Commit, error.to_string()));
        commit_trace::finish_commit(commit_observation, &result);
        result
    }
}
