use crate::context::error::ContextError;
use crate::observability::{
    Attribute, Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus,
};
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_pipeline::TurnStage;

pub fn begin_load_story_snapshot_observation(parent: &Observation, ctx: &TurnExecutionContext) -> Observation {
    begin_observation(parent, ctx, "load-story-snapshot")
}

pub fn begin_activate_world_info_observation(parent: &Observation, ctx: &TurnExecutionContext) -> Observation {
    begin_observation(parent, ctx, "activate-world-info")
}

pub fn finish_observation<T, E>(observation: Observation, outcome: &Result<T, E>)
where
    E: Clone + Into<ContextError>,
{
    observation.finish(context_outcome(outcome, TurnStage::BaselineBuilder));
}

fn begin_observation(parent: &Observation, ctx: &TurnExecutionContext, name: &'static str) -> Observation {
    parent.begin(ObservationSpec {
        name,
        kind: ObservationKind::Retriever,
        input: None,
        metadata: vec![Attribute::string(
            "aise.observation.metadata.story_id",
            ctx.story_id().as_str(),
        )],
    })
}

fn context_outcome<T, E>(outcome: &Result<T, E>, stage: TurnStage) -> ObservationOutcome
where
    E: Clone + Into<ContextError>,
{
    match outcome {
        Ok(_) => ObservationOutcome {
            status: ObservationStatus::Ok,
            ..ObservationOutcome::default()
        },
        Err(error) => {
            let error = error.clone().into();
            ObservationOutcome {
                status: ObservationStatus::Error,
                error: Some(error_info(&error, stage)),
                ..ObservationOutcome::default()
            }
        }
    }
}

fn error_info(error: &ContextError, stage: TurnStage) -> ObservationError {
    ObservationError {
        code: error.turn_code().into(),
        failure_kind: "context".into(),
        stage: Some(stage.as_str().into()),
        message: error.to_string(),
    }
}
