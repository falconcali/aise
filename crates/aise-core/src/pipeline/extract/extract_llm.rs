use crate::core::{ChatMessage, NarrativeConditionId, SemanticQuery, Truth, TurnControl};
use crate::llm::{LlmCompletionSpec, LlmError, LlmGateway};
use crate::pipeline::common::{PipelineError, PipelineStage};
use crate::trace::Observation;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EvaluateNarrativeOutputDto {
    evaluations: Vec<SemanticEvaluationDto>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SemanticEvaluationDto {
    condition_id: NarrativeConditionId,
    truth: Truth,
}

pub(super) async fn evaluate_narrative(
    gateway: &LlmGateway,
    messages: Vec<ChatMessage>,
    control: &TurnControl,
    observation: &Observation,
) -> Result<String, LlmError> {
    gateway
        .complete(LlmCompletionSpec { messages }, control, observation)
        .await
        .map(|result| result.content)
}

pub(super) fn parse_narrative_evaluations(
    raw: &str,
    queries: &[SemanticQuery],
) -> Result<BTreeMap<NarrativeConditionId, Truth>, PipelineError> {
    let output = serde_json::from_str::<EvaluateNarrativeOutputDto>(raw).map_err(|error| {
        PipelineError::invalid_output(
            PipelineStage::Extract,
            "extract_narrative_invalid_json",
            format!(
                "invalid narrative evaluation JSON at line {} column {}",
                error.line(),
                error.column()
            ),
        )
    })?;
    let expected = queries.iter().map(|query| &query.condition_id).collect::<BTreeSet<_>>();
    let mut evaluations = BTreeMap::new();
    for evaluation in output.evaluations {
        if evaluations.contains_key(&evaluation.condition_id) {
            return Err(PipelineError::invalid_output(
                PipelineStage::Extract,
                "extract_narrative_duplicate_condition",
                format!("condition {} was returned more than once", evaluation.condition_id),
            ));
        }
        if !expected.contains(&evaluation.condition_id) {
            return Err(PipelineError::invalid_output(
                PipelineStage::Extract,
                "extract_narrative_unknown_condition",
                format!("condition {} was not requested", evaluation.condition_id),
            ));
        }
        evaluations.insert(evaluation.condition_id, evaluation.truth);
    }
    let missing = queries
        .iter()
        .filter(|query| !evaluations.contains_key(&query.condition_id))
        .map(|query| query.condition_id.as_str())
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(PipelineError::invalid_output(
            PipelineStage::Extract,
            "extract_narrative_missing_condition",
            format!("missing narrative conditions: {}", missing.join(", ")),
        ));
    }
    Ok(evaluations)
}

#[cfg(test)]
#[path = "tests/extract_llm_tests.rs"]
mod tests;
