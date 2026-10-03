use crate::pipeline::common::{Pipeline, PipelineError};
use crate::trace::{Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationStatus, Trace};

pub fn begin_pipeline_observation<P: Pipeline>(trace: &Trace, pipeline: &P) -> Observation {
    trace.begin_observation_with_name(pipeline.stage().as_str(), ObservationKind::Chain)
}

pub fn finish_pipeline_observation<P: Pipeline>(
    observation: Observation,
    pipeline: &P,
    result: &Result<P::Output, PipelineError>,
) {
    let outcome = match &result {
        Ok(_) => ObservationOutcome {
            status: ObservationStatus::Ok,
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
