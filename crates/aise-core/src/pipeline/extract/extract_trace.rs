use crate::core::{NarrativeConditionId, RoleId, RoleMemoryChangeOp, SemanticQuery, Truth};
use crate::pipeline::common::PipelineError;
use crate::pipeline::extract::extract_knowledge::WorldKnowledgeExtraction;
use crate::trace::{
    Attribute, Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus,
};
use std::collections::BTreeMap;

pub(super) fn begin_evaluate_narrative(
    observation: &Observation,
    proposal: &str,
    queries: &[SemanticQuery],
) -> Observation {
    observation.begin(ObservationSpec {
        name: "extract_evaluate_narrative",
        kind: ObservationKind::Span,
        input: observation.capture_content(&(proposal, queries)),
        metadata: Vec::new(),
    })
}

pub(super) fn finish_evaluate_narrative(
    observation: Observation,
    queries: &[SemanticQuery],
    result: &Result<BTreeMap<NarrativeConditionId, Truth>, PipelineError>,
) {
    let outcome = match result {
        Ok(evaluations) => ObservationOutcome {
            status: ObservationStatus::Ok,
            output: observation.capture_content(&(queries, evaluations)),
            ..ObservationOutcome::default()
        },
        Err(error) => ObservationOutcome {
            status: ObservationStatus::Error,
            error: Some(observation_error(error)),
            ..ObservationOutcome::default()
        },
    };
    observation.finish(outcome);
}

pub(super) fn begin_extract_world_knowledge(observation: &Observation, proposal: &str) -> Observation {
    observation.begin(ObservationSpec {
        name: "extract_world_knowledge",
        kind: ObservationKind::Span,
        input: observation.capture_content(&proposal),
        metadata: Vec::new(),
    })
}

pub(super) fn finish_extract_world_knowledge(
    observation: Observation,
    result: &Result<WorldKnowledgeExtraction, PipelineError>,
) {
    let outcome = match result {
        Ok(extraction) => ObservationOutcome {
            status: ObservationStatus::Ok,
            output: observation.capture_content(extraction),
            ..ObservationOutcome::default()
        },
        Err(error) => ObservationOutcome {
            status: ObservationStatus::Error,
            error: Some(observation_error(error)),
            ..ObservationOutcome::default()
        },
    };
    observation.finish(outcome);
}

pub(super) fn begin_extract_role_memory(observation: &Observation, role_id: &RoleId, proposal: &str) -> Observation {
    observation.begin(ObservationSpec {
        name: "extract_role_memory",
        kind: ObservationKind::Span,
        input: observation.capture_content(&(role_id, proposal)),
        metadata: vec![Attribute::string("role_id", role_id.as_str())],
    })
}

pub(super) fn finish_extract_role_memory(
    observation: Observation,
    result: &Result<Vec<RoleMemoryChangeOp>, PipelineError>,
) {
    let outcome = match result {
        Ok(changes) => ObservationOutcome {
            status: ObservationStatus::Ok,
            output: observation.capture_content(changes),
            ..ObservationOutcome::default()
        },
        Err(error) => ObservationOutcome {
            status: ObservationStatus::Error,
            error: Some(observation_error(error)),
            ..ObservationOutcome::default()
        },
    };
    observation.finish(outcome);
}

fn observation_error(error: &PipelineError) -> ObservationError {
    let (code, failure_kind) = match error {
        PipelineError::InvalidOutput { code, .. } => (*code, "invalid_output"),
        PipelineError::InconsistentData { code, .. } => (*code, "inconsistent_data"),
        PipelineError::Llm { source, .. } => (source.kind(), "llm"),
        PipelineError::Prompt { .. } => ("prompt_error", "prompt"),
        PipelineError::Persistence { .. } => ("persistence_error", "persistence"),
        PipelineError::Narrative { .. } => ("narrative_error", "narrative"),
        PipelineError::EmptyOutput { .. } => ("empty_output", "pipeline"),
        PipelineError::InvalidConfig { code, .. } => (*code, "invalid_config"),
        PipelineError::ValidationFailed { code, .. } => (*code, "validation"),
    };
    ObservationError {
        code: code.to_owned(),
        failure_kind: failure_kind.to_owned(),
        stage: Some("extract".to_owned()),
        message: error.to_string(),
    }
}

#[cfg(test)]
#[path = "tests/extract_trace_tests.rs"]
mod tests;
