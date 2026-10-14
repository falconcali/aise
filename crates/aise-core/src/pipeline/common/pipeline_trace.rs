use crate::pipeline::common::{Pipeline, PipelineError};
use crate::trace::{
    Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus, Trace,
};
use serde::Serialize;

pub fn begin_observation<I: Send + Serialize, P: Pipeline<I>>(trace: &Trace, pipeline: &P, input: &I) -> Observation {
    trace.begin_observation(ObservationSpec {
        name: pipeline.stage().as_str(),
        kind: ObservationKind::Chain,
        input: trace.root().capture_content(input),
        metadata: Vec::new(),
    })
}

pub fn finish_observation<I: Send + Serialize, P: Pipeline<I>>(
    observation: Observation,
    pipeline: &P,
    result: &Result<P::Output, PipelineError>,
) {
    let outcome = match result {
        Ok(output) => ObservationOutcome {
            status: ObservationStatus::Ok,
            output: observation.capture_content(output),
            ..ObservationOutcome::default()
        },
        Err(error) => ObservationOutcome {
            status: ObservationStatus::Error,
            error: Some(ObservationError {
                code: "pipeline_stage_failed".into(),
                failure_kind: "pipeline".into(),
                stage: Some(pipeline.stage().as_str().into()),
                message: error.to_string(),
            }),
            ..ObservationOutcome::default()
        },
    };
    observation.finish(outcome);
}
