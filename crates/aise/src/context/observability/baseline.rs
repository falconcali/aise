use crate::context::error::ContextError;
use crate::observability::{
    Attribute, Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus,
};
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_pipeline::TurnStage;

pub struct LoadStorySnapshotObservation {
    observation: Observation,
}

pub fn begin_load_story_snapshot(parent: &Observation, ctx: &TurnExecutionContext) -> LoadStorySnapshotObservation {
    LoadStorySnapshotObservation {
        observation: parent.begin(ObservationSpec {
            name: "load-story-snapshot",
            kind: ObservationKind::Retriever,
            input: None,
            metadata: vec![Attribute::string(
                "aise.observation.metadata.story_id",
                ctx.story_id().as_str(),
            )],
        }),
    }
}

impl LoadStorySnapshotObservation {
    pub fn finish<T, E>(self, outcome: &Result<T, E>)
    where
        E: Clone + Into<ContextError>,
    {
        self.observation.finish(context_outcome(outcome, TurnStage::BaselineBuilder));
    }
}

pub struct ActivateWorldInfoObservation {
    observation: Observation,
}

pub fn begin_activate_world_info(parent: &Observation, ctx: &TurnExecutionContext) -> ActivateWorldInfoObservation {
    ActivateWorldInfoObservation {
        observation: parent.begin(ObservationSpec {
            name: "activate-world-info",
            kind: ObservationKind::Retriever,
            input: None,
            metadata: vec![Attribute::string(
                "aise.observation.metadata.story_id",
                ctx.story_id().as_str(),
            )],
        }),
    }
}

impl ActivateWorldInfoObservation {
    pub fn finish<T, E>(self, outcome: &Result<T, E>)
    where
        E: Clone + Into<ContextError>,
    {
        self.observation.finish(context_outcome(outcome, TurnStage::BaselineBuilder));
    }
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
