use crate::pipeline::common::PipelineError;
use crate::pipeline::plan::PlanOutput;
use crate::trace::{
    Attribute, Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus,
};

const FACT_CANDIDATES_TOTAL: &str = "aise.plan.retrieval.fact_candidates_total";
const FACT_CANDIDATES_OFFERED: &str = "aise.plan.retrieval.fact_candidates_offered";
const FACT_CANDIDATES_DROPPED: &str = "aise.plan.retrieval.fact_candidates_dropped";
const FACT_CANDIDATES_TRUNCATED: &str = "aise.plan.retrieval.fact_candidates_truncated";
const RUMOR_CANDIDATES_TOTAL: &str = "aise.plan.retrieval.rumor_candidates_total";
const RUMOR_CANDIDATES_OFFERED: &str = "aise.plan.retrieval.rumor_candidates_offered";
const RUMOR_CANDIDATES_DROPPED: &str = "aise.plan.retrieval.rumor_candidates_dropped";
const RUMOR_CANDIDATES_TRUNCATED: &str = "aise.plan.retrieval.rumor_candidates_truncated";
const FACTS_SELECTED: &str = "aise.plan.retrieval.facts_selected";
const RUMORS_SELECTED: &str = "aise.plan.retrieval.rumors_selected";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlanTraceMetrics {
    pub fact_candidates_total: usize,
    pub fact_candidates_offered: usize,
    pub rumor_candidates_total: usize,
    pub rumor_candidates_offered: usize,
    pub facts_selected: usize,
    pub rumors_selected: usize,
}

pub fn begin_plan(observation: &Observation, player_input: &str) -> Observation {
    observation.begin(ObservationSpec {
        name: "plan",
        kind: ObservationKind::Span,
        input: observation.capture_content(&player_input),
        metadata: Vec::new(),
    })
}

pub fn warn_if_candidates_truncated(metrics: &PlanTraceMetrics) {
    let fact_candidates_dropped = metrics.fact_candidates_total.saturating_sub(metrics.fact_candidates_offered);
    let rumor_candidates_dropped = metrics.rumor_candidates_total.saturating_sub(metrics.rumor_candidates_offered);
    if candidates_truncated(metrics) {
        tracing::warn!(
            target: "aise::pipeline::plan",
            fact_candidates_total = metrics.fact_candidates_total,
            fact_candidates_offered = metrics.fact_candidates_offered,
            fact_candidates_dropped,
            rumor_candidates_total = metrics.rumor_candidates_total,
            rumor_candidates_offered = metrics.rumor_candidates_offered,
            rumor_candidates_dropped,
            "plan retrieval candidates truncated"
        );
    }
}

fn candidates_truncated(metrics: &PlanTraceMetrics) -> bool {
    metrics.fact_candidates_total > metrics.fact_candidates_offered
        || metrics.rumor_candidates_total > metrics.rumor_candidates_offered
}

pub fn finish_plan(observation: Observation, result: &Result<PlanOutput, PipelineError>, metrics: &PlanTraceMetrics) {
    let outcome = match result {
        Ok(output) => ObservationOutcome {
            status: ObservationStatus::Ok,
            metadata: retrieval_metadata(metrics),
            output: observation.capture_content(output),
            ..ObservationOutcome::default()
        },
        Err(error) => ObservationOutcome {
            status: ObservationStatus::Error,
            metadata: retrieval_metadata(metrics),
            error: Some(observation_error(error)),
            ..ObservationOutcome::default()
        },
    };
    observation.finish(outcome);
}

fn retrieval_metadata(metrics: &PlanTraceMetrics) -> Vec<Attribute> {
    let fact_candidates_dropped = metrics.fact_candidates_total.saturating_sub(metrics.fact_candidates_offered);
    let rumor_candidates_dropped = metrics.rumor_candidates_total.saturating_sub(metrics.rumor_candidates_offered);
    vec![
        Attribute::u64(FACT_CANDIDATES_TOTAL, metrics.fact_candidates_total as u64),
        Attribute::u64(FACT_CANDIDATES_OFFERED, metrics.fact_candidates_offered as u64),
        Attribute::u64(FACT_CANDIDATES_DROPPED, fact_candidates_dropped as u64),
        Attribute::bool(FACT_CANDIDATES_TRUNCATED, fact_candidates_dropped > 0),
        Attribute::u64(RUMOR_CANDIDATES_TOTAL, metrics.rumor_candidates_total as u64),
        Attribute::u64(RUMOR_CANDIDATES_OFFERED, metrics.rumor_candidates_offered as u64),
        Attribute::u64(RUMOR_CANDIDATES_DROPPED, rumor_candidates_dropped as u64),
        Attribute::bool(RUMOR_CANDIDATES_TRUNCATED, rumor_candidates_dropped > 0),
        Attribute::u64(FACTS_SELECTED, metrics.facts_selected as u64),
        Attribute::u64(RUMORS_SELECTED, metrics.rumors_selected as u64),
    ]
}

fn observation_error(error: &PipelineError) -> ObservationError {
    let (code, failure_kind) = match error {
        PipelineError::InvalidOutput { code, .. } => (*code, "invalid_output"),
        PipelineError::Llm { .. } => ("llm_error", "llm"),
        PipelineError::Prompt { .. } => ("prompt_error", "prompt"),
        PipelineError::Persistence { .. } => ("persistence_error", "persistence"),
        PipelineError::Narrative { .. }
        | PipelineError::EmptyOutput { .. }
        | PipelineError::InconsistentData { .. } => ("pipeline_error", "pipeline"),
    };
    ObservationError {
        code: code.into(),
        failure_kind: failure_kind.into(),
        stage: Some("plan".into()),
        message: error.to_string(),
    }
}

#[cfg(test)]
#[path = "tests/plan_trace_tests.rs"]
mod tests;
