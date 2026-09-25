use crate::domain::ids::TurnNumber;
use crate::observability::{Attribute, Observation, ObservationError, ObservationOutcome, ObservationStatus, Trace};
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};

pub struct EngineObservation {
    observation: Observation,
}

impl EngineObservation {
    pub fn finish_ok(self) {
        self.observation.finish(ObservationOutcome {
            status: ObservationStatus::Ok,
            ..ObservationOutcome::default()
        });
    }

    pub fn finish_error(self, error: &TurnExecutionError) {
        self.observation.finish(execution_error(error));
    }

    pub fn finish_story_not_found(self) {
        self.observation.finish(ObservationOutcome {
            status: ObservationStatus::Error,
            error: Some(ObservationError {
                code: "story_not_found".into(),
                failure_kind: "story_not_found".into(),
                stage: None,
                message: "story not found".into(),
            }),
            ..ObservationOutcome::default()
        });
    }
}

pub fn begin_coordinate_story_turn(trace: &Trace) -> EngineObservation {
    begin(trace, "coordinate-story-turn")
}

pub fn begin_load_story(trace: &Trace) -> EngineObservation {
    begin(trace, "load-story")
}

pub fn begin_check_idempotency(trace: &Trace) -> EngineObservation {
    begin(trace, "check-idempotency")
}

pub fn bind_turn_number(trace: &mut Trace, turn_number: TurnNumber) {
    trace.bind(vec![Attribute::u64(
        crate::observability::TRACE_METADATA_TURN_NUMBER,
        turn_number.get(),
    )]);
}

fn begin(trace: &Trace, name: &'static str) -> EngineObservation {
    EngineObservation {
        observation: trace.begin_observation(crate::observability::ObservationSpec {
            name,
            kind: crate::observability::ObservationKind::Chain,
            input: None,
            metadata: Vec::new(),
        }),
    }
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
