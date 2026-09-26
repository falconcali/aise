use crate::observability::{
    Attribute, Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus,
};
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};

pub fn begin_plan_turn(parent: &Observation, ctx: &TurnExecutionContext) -> Observation {
    begin_context(parent, ctx, "plan-turn")
}

pub fn begin_project_narrative(parent: &Observation, ctx: &TurnExecutionContext) -> Observation {
    begin_context(parent, ctx, "project-narrative")
}

pub fn begin_generate_writer_plan(parent: &Observation) -> Observation {
    parent.begin(ObservationSpec {
        name: "generate-writer-plan",
        kind: ObservationKind::Chain,
        input: None,
        metadata: Vec::new(),
    })
}

pub fn finish(observation: Observation, outcome: &Result<(), TurnExecutionError>) {
    observation.finish(turn_outcome(outcome));
}

pub fn end_project_narrative(observation: Observation, graph_revision: u64, node_count: usize, edge_count: usize) {
    observation.finish(ObservationOutcome {
        status: ObservationStatus::Ok,
        metadata: vec![
            Attribute::u64("aise.observation.metadata.graph_revision", graph_revision),
            Attribute::u64("aise.observation.metadata.projected_node_count", node_count as u64),
            Attribute::u64("aise.observation.metadata.projected_edge_count", edge_count as u64),
        ],
        ..ObservationOutcome::default()
    });
}

fn begin_context(parent: &Observation, ctx: &TurnExecutionContext, name: &'static str) -> Observation {
    parent.begin(ObservationSpec {
        name,
        kind: ObservationKind::Chain,
        input: None,
        metadata: vec![Attribute::string(
            "aise.observation.metadata.story_id",
            ctx.story_id().as_str(),
        )],
    })
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
