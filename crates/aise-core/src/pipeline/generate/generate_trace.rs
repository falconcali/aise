use crate::pipeline::common::PipelineError;
use crate::trace::{
    Attribute, Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus,
};

const GENERATE_ROLES: &str = "aise.generate.roles";
const GENERATE_CHARACTER_DECISIONS: &str = "aise.generate.character_decisions";
const GENERATE_ROLE_CONTEXT_BYTES: &str = "aise.generate.role_context_bytes";
const GENERATE_DECISION_CONTEXT_BYTES: &str = "aise.generate.decision_context_bytes";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GenerateTraceMetrics {
    pub roles: usize,
    pub character_decisions: usize,
    pub role_context_bytes: usize,
    pub decision_context_bytes: usize,
}

pub fn begin_story_generation(observation: &Observation, story_goal: &str, player_contribution: &str) -> Observation {
    observation.begin(ObservationSpec {
        name: "story_generation",
        kind: ObservationKind::Span,
        input: observation.capture_content(&(story_goal, player_contribution)),
        metadata: Vec::new(),
    })
}

pub fn finish_story_generation(
    observation: Observation,
    result: &Result<String, PipelineError>,
    metrics: &GenerateTraceMetrics,
) {
    let outcome = match result {
        Ok(story_segment) => ObservationOutcome {
            status: ObservationStatus::Ok,
            metadata: generate_metadata(metrics),
            output: observation.capture_content(story_segment),
            ..ObservationOutcome::default()
        },
        Err(error) => ObservationOutcome {
            status: ObservationStatus::Error,
            metadata: generate_metadata(metrics),
            error: Some(observation_error(error)),
            ..ObservationOutcome::default()
        },
    };
    observation.finish(outcome);
}

fn generate_metadata(metrics: &GenerateTraceMetrics) -> Vec<Attribute> {
    vec![
        Attribute::u64(GENERATE_ROLES, metrics.roles as u64),
        Attribute::u64(GENERATE_CHARACTER_DECISIONS, metrics.character_decisions as u64),
        Attribute::u64(GENERATE_ROLE_CONTEXT_BYTES, metrics.role_context_bytes as u64),
        Attribute::u64(GENERATE_DECISION_CONTEXT_BYTES, metrics.decision_context_bytes as u64),
    ]
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
    };
    ObservationError {
        code: code.to_owned(),
        failure_kind: failure_kind.to_owned(),
        stage: Some("generate".to_owned()),
        message: error.to_string(),
    }
}
