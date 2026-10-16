use crate::core::{RoleActor, StoryContext};
use crate::pipeline::common::PipelineError;
use crate::pipeline::retrieval::{RetrievalInput, RetrievalOutput};
use crate::trace::{
    Attribute, Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus,
};

const FACTS_REQUESTED: &str = "aise.retrieval.facts_requested";
const FACTS_RESOLVED: &str = "aise.retrieval.facts_resolved";
const RUMORS_REQUESTED: &str = "aise.retrieval.rumors_requested";
const RUMORS_RESOLVED: &str = "aise.retrieval.rumors_resolved";
const ROLES_REQUESTED: &str = "aise.retrieval.roles_requested";
const ROLES_RESOLVED: &str = "aise.retrieval.roles_resolved";
const DEFAULT_ROLES_RESOLVED: &str = "aise.retrieval.roles_default";
const CARD_ROLES_RESOLVED: &str = "aise.retrieval.roles_card";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RetrievalTraceMetrics {
    pub facts_requested: usize,
    pub facts_resolved: usize,
    pub rumors_requested: usize,
    pub rumors_resolved: usize,
    pub roles_requested: usize,
    pub roles_resolved: usize,
    pub default_roles_resolved: usize,
    pub card_roles_resolved: usize,
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
    story_ctx: &StoryContext,
    result: &Result<RetrievalOutput, PipelineError>,
) {
    let (default_roles_resolved, card_roles_resolved) = result
        .as_ref()
        .map_or((0, 0), |output| role_kind_counts(story_ctx, input, output));
    let metrics = RetrievalTraceMetrics {
        facts_requested: input.fact_ids.len(),
        facts_resolved: result.as_ref().map_or(0, |output| output.facts.len()),
        rumors_requested: input.rumor_ids.len(),
        rumors_resolved: result.as_ref().map_or(0, |output| output.rumors.len()),
        roles_requested: input.role_ids.len(),
        roles_resolved: result.as_ref().map_or(0, |output| output.roles.len()),
        default_roles_resolved,
        card_roles_resolved,
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
        Attribute::u64(ROLES_REQUESTED, metrics.roles_requested as u64),
        Attribute::u64(ROLES_RESOLVED, metrics.roles_resolved as u64),
        Attribute::u64(DEFAULT_ROLES_RESOLVED, metrics.default_roles_resolved as u64),
        Attribute::u64(CARD_ROLES_RESOLVED, metrics.card_roles_resolved as u64),
    ]
}

fn role_kind_counts(story_ctx: &StoryContext, input: &RetrievalInput<'_>, output: &RetrievalOutput) -> (usize, usize) {
    input
        .role_ids
        .iter()
        .filter_map(|role_id| output.role(role_id))
        .fold((0, 0), |(defaults, cards), role| match story_ctx.cast.get(&role.role_id) {
            Some(RoleActor::Default) => (defaults + 1, cards),
            Some(RoleActor::Card(_)) => (defaults, cards + 1),
            None => (defaults, cards),
        })
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
        PipelineError::InvalidConfig { code, .. } => (*code, "invalid_config"),
        PipelineError::ValidationFailed { code, .. } => (*code, "validation"),
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
