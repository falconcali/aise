use crate::pipeline::common::PipelineError;
use crate::pipeline::repair::RepairInput;
use crate::trace::{
    Attribute, Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus,
};

const INPUT_PROPOSAL_VERSION: &str = "aise.repair.input_proposal_version";
const OUTPUT_PROPOSAL_VERSION: &str = "aise.repair.output_proposal_version";
const ROUND: &str = "aise.repair.round";
const ROUNDS_REMAINING: &str = "aise.repair.rounds_remaining";
const ISSUES: &str = "aise.repair.issues";
const OUTPUT_BYTES: &str = "aise.repair.output_bytes";

pub(super) fn begin_repair(observation: &Observation, input: &RepairInput<'_>) -> Observation {
    observation.begin(ObservationSpec {
        name: "repair_generation",
        kind: ObservationKind::Span,
        input: observation.capture_content(&(input.proposal_version, input.proposal, input.issues)),
        metadata: Vec::new(),
    })
}

pub(super) fn finish_repair(observation: Observation, input: &RepairInput<'_>, result: &Result<String, PipelineError>) {
    let outcome = match result {
        Ok(proposal) => ObservationOutcome {
            status: ObservationStatus::Ok,
            metadata: repair_metadata(input, proposal.len()),
            output: observation.capture_content(proposal),
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

fn repair_metadata(input: &RepairInput<'_>, output_bytes: usize) -> Vec<Attribute> {
    vec![
        Attribute::u64(INPUT_PROPOSAL_VERSION, u64::from(input.proposal_version)),
        Attribute::u64(OUTPUT_PROPOSAL_VERSION, u64::from(input.proposal_version) + 1),
        Attribute::u64(ROUND, u64::from(input.repair_round)),
        Attribute::u64(ROUNDS_REMAINING, u64::from(input.repair_rounds_remaining)),
        Attribute::u64(ISSUES, input.issues.len() as u64),
        Attribute::u64(OUTPUT_BYTES, output_bytes as u64),
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
        PipelineError::InvalidConfig { code, .. } => (*code, "invalid_config"),
        PipelineError::ValidationFailed { code, .. } => (*code, "validation"),
    };
    ObservationError {
        code: code.to_owned(),
        failure_kind: failure_kind.to_owned(),
        stage: Some("repair".to_owned()),
        message: error.to_string(),
    }
}

#[cfg(test)]
#[path = "tests/repair_trace_tests.rs"]
mod tests;
