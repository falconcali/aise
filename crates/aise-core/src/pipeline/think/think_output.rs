use crate::core::{BoundedText, CharacterDecision, RoleId};
use crate::pipeline::common::{PipelineError, PipelineStage};
use crate::pipeline::think::ThinkConfig;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CharacterDecisionDto {
    decision: String,
    #[serde(default)]
    suggested_utterance: Option<String>,
}

pub(super) fn parse_character_decision(
    raw: &str,
    role_id: &RoleId,
    config: &ThinkConfig,
) -> Result<CharacterDecision, PipelineError> {
    let dto: CharacterDecisionDto = serde_json::from_str(raw).map_err(|error| {
        PipelineError::invalid_output(PipelineStage::Think, "think_output_invalid_json", error.to_string())
    })?;
    let decision = dto.decision.trim();
    if decision.is_empty() {
        return Err(invalid("think_decision_empty", "decision must not be empty"));
    }
    if decision.len() > config.max_decision_bytes {
        return Err(invalid(
            "think_decision_too_long",
            format!("decision size {} exceeds maximum {}", decision.len(), config.max_decision_bytes),
        ));
    }
    let suggested_utterance = dto.suggested_utterance.as_deref().map(str::trim);
    if suggested_utterance == Some("") {
        return Err(invalid(
            "think_suggested_utterance_empty",
            "suggested utterance must not be empty",
        ));
    }
    if suggested_utterance.is_some_and(|value| value.len() > config.max_suggested_utterance_bytes) {
        return Err(invalid(
            "think_suggested_utterance_too_long",
            format!(
                "suggested utterance size {} exceeds maximum {}",
                suggested_utterance.map_or(0, str::len),
                config.max_suggested_utterance_bytes
            ),
        ));
    }
    let total_bytes = decision.len() + suggested_utterance.map_or(0, str::len);
    if total_bytes > config.max_total_output_bytes {
        return Err(invalid(
            "think_output_too_long",
            format!(
                "combined output size {total_bytes} exceeds maximum {}",
                config.max_total_output_bytes
            ),
        ));
    }
    let decision = bounded(decision, "decision", config.max_decision_bytes, "think_decision_too_long")?;
    let suggested_utterance = suggested_utterance
        .map(|value| {
            bounded(
                value,
                "suggested_utterance",
                config.max_suggested_utterance_bytes,
                "think_suggested_utterance_too_long",
            )
        })
        .transpose()?;
    Ok(CharacterDecision {
        role_id: role_id.clone(),
        decision,
        suggested_utterance,
    })
}

fn bounded(
    value: &str,
    field: &'static str,
    maximum_bytes: usize,
    code: &'static str,
) -> Result<BoundedText, PipelineError> {
    BoundedText::try_new(value, field, maximum_bytes)
        .map_err(|error| PipelineError::invalid_output(PipelineStage::Think, code, error.to_string()))
}

fn invalid(code: &'static str, message: impl Into<String>) -> PipelineError {
    PipelineError::invalid_output(PipelineStage::Think, code, message)
}

#[cfg(test)]
#[path = "tests/think_output_tests.rs"]
mod tests;
