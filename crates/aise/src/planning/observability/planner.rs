use crate::observability::{
    Attribute, Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus,
};
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};

pub struct PlanTurnObservation {
    observation: Observation,
}

pub fn begin_plan_turn(parent: &Observation, ctx: &TurnExecutionContext) -> PlanTurnObservation {
    PlanTurnObservation {
        observation: parent.begin(ObservationSpec {
            name: "plan-turn",
            kind: ObservationKind::Chain,
            input: None,
            metadata: vec![Attribute::string(
                "aise.observation.metadata.story_id",
                ctx.story_id().as_str(),
            )],
        }),
    }
}

impl PlanTurnObservation {
    pub fn observation(&self) -> &Observation {
        &self.observation
    }

    pub fn finish(self, outcome: &Result<(), TurnExecutionError>) {
        self.observation.finish(turn_outcome(outcome));
    }
}

pub struct ProjectNarrativeObservation {
    observation: Observation,
}

pub fn begin_project_narrative(parent: &Observation, ctx: &TurnExecutionContext) -> ProjectNarrativeObservation {
    ProjectNarrativeObservation {
        observation: parent.begin(ObservationSpec {
            name: "project-narrative",
            kind: ObservationKind::Chain,
            input: None,
            metadata: vec![Attribute::string(
                "aise.observation.metadata.story_id",
                ctx.story_id().as_str(),
            )],
        }),
    }
}

impl ProjectNarrativeObservation {
    pub fn finish(self, graph_revision: u64, node_count: usize, edge_count: usize) {
        self.observation.finish(ObservationOutcome {
            status: ObservationStatus::Ok,
            metadata: vec![
                Attribute::u64("aise.observation.metadata.graph_revision", graph_revision),
                Attribute::u64("aise.observation.metadata.projected_node_count", node_count as u64),
                Attribute::u64("aise.observation.metadata.projected_edge_count", edge_count as u64),
            ],
            ..ObservationOutcome::default()
        });
    }
}

pub struct GenerateWriterPlanObservation {
    observation: Observation,
}

pub fn begin_generate_writer_plan(parent: &Observation) -> GenerateWriterPlanObservation {
    GenerateWriterPlanObservation {
        observation: parent.begin(ObservationSpec {
            name: "generate-writer-plan",
            kind: ObservationKind::Chain,
            input: None,
            metadata: Vec::new(),
        }),
    }
}

impl GenerateWriterPlanObservation {
    pub fn observation(&self) -> &Observation {
        &self.observation
    }

    pub fn finish(self, outcome: &Result<(), TurnExecutionError>) {
        self.observation.finish(turn_outcome(outcome));
    }
}

fn turn_outcome(outcome: &Result<(), TurnExecutionError>) -> ObservationOutcome {
    match outcome {
        Ok(()) => ObservationOutcome {
            status: ObservationStatus::Ok,
            ..ObservationOutcome::default()
        },
        Err(error) => ObservationOutcome {
            status: status(error),
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

fn status(error: &TurnExecutionError) -> ObservationStatus {
    match error.kind() {
        TurnFailureKind::Cancelled => ObservationStatus::Cancelled,
        TurnFailureKind::DeadlineExceeded => ObservationStatus::DeadlineExceeded,
        TurnFailureKind::RevisionConflict | TurnFailureKind::IdempotencyConflict => ObservationStatus::Conflict,
        _ => ObservationStatus::Error,
    }
}
