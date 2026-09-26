use crate::observability::{
    Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus,
};
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};

pub fn begin_generate_story(parent: &Observation, ctx: &TurnExecutionContext) -> Observation {
    begin(parent, ctx, "generate-story", ObservationKind::Chain)
}

pub fn begin_draft_story_text(parent: &Observation, ctx: &TurnExecutionContext) -> Observation {
    begin(parent, ctx, "draft-story-text", ObservationKind::Generation)
}

pub fn finish(observation: Observation, outcome: &Result<(), TurnExecutionError>) {
    let observation_outcome = outcome_for(&observation, outcome);
    observation.finish(observation_outcome);
}

fn begin(parent: &Observation, ctx: &TurnExecutionContext, name: &'static str, kind: ObservationKind) -> Observation {
    parent.begin(ObservationSpec {
        name,
        kind,
        input: parent.capture_content(&ctx.observation_input()),
        metadata: Vec::new(),
    })
}

fn outcome_for(observation: &Observation, outcome: &Result<(), TurnExecutionError>) -> ObservationOutcome {
    match outcome {
        Ok(()) => ObservationOutcome {
            status: ObservationStatus::Ok,
            output: observation.capture_content(&serde_json::json!({"status": "generated"})),
            ..ObservationOutcome::default()
        },
        Err(error) => ObservationOutcome {
            status: match error.kind() {
                TurnFailureKind::Cancelled => ObservationStatus::Cancelled,
                TurnFailureKind::DeadlineExceeded => ObservationStatus::DeadlineExceeded,
                TurnFailureKind::RevisionConflict | TurnFailureKind::IdempotencyConflict => ObservationStatus::Conflict,
                _ => ObservationStatus::Error,
            },
            error: Some(ObservationError {
                code: error.code().into(),
                failure_kind: format!("{:?}", error.kind()).to_lowercase(),
                stage: error.stage().map(|stage| stage.as_str().into()),
                message: error.to_string(),
            }),
            ..ObservationOutcome::default()
        },
    }
}
