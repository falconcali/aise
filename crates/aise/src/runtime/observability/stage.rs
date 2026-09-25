use crate::observability::{Observation, ObservationError, ObservationOutcome, ObservationStatus};
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};
use crate::turn::turn_pipeline::TurnStage;

pub struct PipelineStageObservation {
    observation: Observation,
}

impl PipelineStageObservation {
    pub fn begin(parent: &Observation, stage: TurnStage) -> Self {
        Self {
            observation: parent.begin(crate::observability::ObservationSpec {
                name: stage_name(stage),
                kind: crate::observability::ObservationKind::Chain,
                input: None,
                metadata: Vec::new(),
            }),
        }
    }

    pub fn observation(&self) -> &Observation {
        &self.observation
    }

    pub fn finish(self, result: &Result<(), TurnExecutionError>) {
        self.observation.finish(match result {
            Ok(()) => ObservationOutcome {
                status: ObservationStatus::Ok,
                ..ObservationOutcome::default()
            },
            Err(error) => ObservationOutcome {
                status: status(error),
                error: Some(error_info(error)),
                ..ObservationOutcome::default()
            },
        });
    }
}

const fn stage_name(stage: TurnStage) -> &'static str {
    match stage {
        TurnStage::TurnInitializer => "initialize-turn",
        TurnStage::BaselineBuilder => "prepare-context",
        TurnStage::WriterPlanner => "plan-turn",
        TurnStage::ContextRetrieval => "retrieve-context",
        TurnStage::CharacterThink => "think-characters",
        TurnStage::StoryGenerator => "generate-story",
        TurnStage::StoryStateExtractor => "extract-story-state",
        TurnStage::Validation => "validate-story",
        TurnStage::StoryRepairer => "repair-story",
        TurnStage::TurnCommitter => "commit-turn",
        TurnStage::Context => "prepare-context",
    }
}

fn error_info(error: &TurnExecutionError) -> ObservationError {
    ObservationError {
        code: error.code().into(),
        failure_kind: format!("{:?}", error.kind()).to_lowercase(),
        stage: error.stage().map(|stage| stage.as_str().into()),
        message: error.to_string(),
    }
}

fn status(error: &TurnExecutionError) -> ObservationStatus {
    match error.kind() {
        TurnFailureKind::Cancelled => ObservationStatus::Cancelled,
        TurnFailureKind::DeadlineExceeded => ObservationStatus::DeadlineExceeded,
        TurnFailureKind::RevisionConflict | TurnFailureKind::IdempotencyConflict => ObservationStatus::Conflict,
        _ => ObservationStatus::Error,
    }
}
