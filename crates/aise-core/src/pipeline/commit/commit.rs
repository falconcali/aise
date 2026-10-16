use crate::core::{
    Change, IdempotencyKey, NarrativeState, PlayerContribution, StoryCommit, StoryContext, StoryLifeCycle,
    StorySummary, Turn, TurnControl, TurnEvaluation, TurnNumber, TurnSegment, TurnStatus, WorldChange,
};
use crate::persistence::Store;
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
    pub turn_status: TurnStatus,
    pub turn_evaluation: TurnEvaluation,
    pub summary: Change<StorySummary>,
    pub narrative: Change<NarrativeState>,
    pub life_cycle: Change<StoryLifeCycle>,
}

pub struct CommitPipeline {
    store: Arc<Store>,
}

impl CommitPipeline {
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }
}

impl Pipeline<CommitInput> for CommitPipeline {
    type Output = StoryCommit;

    fn stage(&self) -> PipelineStage {
        PipelineStage::Commit
    }

    async fn execute(
        &self,
        input: CommitInput,
        story_ctx: &StoryContext,
        _control: &TurnControl,
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
                turn_status: input.turn_status,
            },
            summary: input.summary,
            narrative: input.narrative,
            life_cycle: input.life_cycle,
        };

        let commit_observation = commit_trace::begin_commit(observation, &story_commit);
        let result = self
            .store
            .story_store()
            .commit(story_commit)
            .await
            .map_err(PipelineError::persistence(PipelineStage::Commit));
        commit_trace::finish_commit(commit_observation, &result);
        result
    }
}

#[cfg(test)]
#[path = "tests/commit_tests.rs"]
mod tests;
