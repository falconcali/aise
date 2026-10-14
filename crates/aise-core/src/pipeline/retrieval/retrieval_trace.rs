use crate::pipeline::common::PipelineError;
use crate::pipeline::retrieval::{RetrievalInput, RetrievalOutput};
use crate::trace::{
    Attribute, Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus,
};

const FACTS_REQUESTED: &str = "aise.retrieval.facts_requested";
const FACTS_RESOLVED: &str = "aise.retrieval.facts_resolved";
const RUMORS_REQUESTED: &str = "aise.retrieval.rumors_requested";
const RUMORS_RESOLVED: &str = "aise.retrieval.rumors_resolved";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RetrievalTraceMetrics {
    pub facts_requested: usize,
    pub facts_resolved: usize,
    pub rumors_requested: usize,
    pub rumors_resolved: usize,
}

pub fn begin_retrieval(observation: &Observation, input: &RetrievalInput<'_>) -> Observation {
    observation.begin(ObservationSpec {
        name: "retrieve_knowledge",
        kind: ObservationKind::Retriever,
        input: observation.capture_content(input),
        metadata: Vec::new(),
    })
}

pub fn finish_retrieval(
    observation: Observation,
    input: &RetrievalInput<'_>,
    result: &Result<RetrievalOutput, PipelineError>,
) {
    let metrics = RetrievalTraceMetrics {
        facts_requested: input.fact_ids.len(),
        facts_resolved: result.as_ref().map_or(0, |output| output.facts.len()),
        rumors_requested: input.rumor_ids.len(),
        rumors_resolved: result.as_ref().map_or(0, |output| output.rumors.len()),
    };
    let outcome = match result {
        Ok(output) => ObservationOutcome {
            status: ObservationStatus::Ok,
            metadata: retrieval_metadata(&metrics),
            output: observation.capture_content(output),
            ..ObservationOutcome::default()
        },
        Err(error) => ObservationOutcome {
            status: ObservationStatus::Error,
            metadata: retrieval_metadata(&metrics),
            error: Some(observation_error(error)),
            ..ObservationOutcome::default()
        },
    };
    observation.finish(outcome);
}

fn retrieval_metadata(metrics: &RetrievalTraceMetrics) -> Vec<Attribute> {
    vec![
        Attribute::u64(FACTS_REQUESTED, metrics.facts_requested as u64),
        Attribute::u64(FACTS_RESOLVED, metrics.facts_resolved as u64),
        Attribute::u64(RUMORS_REQUESTED, metrics.rumors_requested as u64),
        Attribute::u64(RUMORS_RESOLVED, metrics.rumors_resolved as u64),
    ]
}

fn observation_error(error: &PipelineError) -> ObservationError {
    let (code, failure_kind) = match error {
        PipelineError::InconsistentData { code, .. } => (*code, "inconsistent_data"),
        PipelineError::Persistence { .. } => ("persistence_error", "persistence"),
        PipelineError::Llm { .. }
        | PipelineError::Prompt { .. }
        | PipelineError::Narrative { .. }
        | PipelineError::EmptyOutput { .. }
        | PipelineError::InvalidOutput { .. } => ("pipeline_error", "pipeline"),
    };
    ObservationError {
        code: code.into(),
        failure_kind: failure_kind.into(),
        stage: Some("retrieval".into()),
        message: error.to_string(),
    }
}

#[cfg(test)]
#[path = "tests/retrieval_trace_tests.rs"]
mod tests;
