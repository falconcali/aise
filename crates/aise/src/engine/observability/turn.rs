use crate::domain::ids::TurnNumber;
use crate::observability::{Attribute, Observation, ObservationError, ObservationOutcome, ObservationStatus, Trace};
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};

pub fn begin_coordinate_story_turn(trace: &Trace) -> Observation {
    begin(trace, "coordinate-story-turn")
}

pub fn begin_load_story(trace: &Trace) -> Observation {
    begin(trace, "load-story")
}

pub fn begin_check_idempotency(trace: &Trace) -> Observation {
    begin(trace, "check-idempotency")
}

pub fn finish<T>(observation: Observation, outcome: &Result<T, TurnExecutionError>) {
    observation.finish(match outcome {
        Ok(_) => ObservationOutcome {
            status: ObservationStatus::Ok,
            ..ObservationOutcome::default()
        },
        Err(error) => execution_error(error),
    });
}

pub fn bind_turn_number(trace: &mut Trace, turn_number: TurnNumber) {
    trace.bind(vec![Attribute::u64(
        crate::observability::TRACE_METADATA_TURN_NUMBER,
        turn_number.get(),
    )]);
}

fn begin(trace: &Trace, name: &'static str) -> Observation {
    trace.begin_observation(crate::observability::ObservationSpec {
        name,
        kind: crate::observability::ObservationKind::Chain,
        input: None,
        metadata: Vec::new(),
    })
}

fn execution_error(error: &TurnExecutionError) -> ObservationOutcome {
    ObservationOutcome {
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
    }
}
