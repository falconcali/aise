use crate::llm::{LlmCompletionRequest, LlmCompletionResponse, LlmError, LlmTokenUsage};
use crate::trace::{
    Attribute, GenerationUsage, OBSERVATION_MODEL_NAME, OBSERVATION_MODEL_PARAMETERS, Observation, ObservationError,
    ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus,
};

pub fn begin_provider_call(observation: &Observation, request: &LlmCompletionRequest) -> Observation {
    let parameters = serde_json::json!({
        "reasoning_effort": request.reasoning_effort,
        "temperature": request.temperature,
    })
    .to_string();
    observation.begin(ObservationSpec {
        name: "llm_provider_call",
        kind: ObservationKind::Generation,
        input: observation.capture_content(&request.messages),
        metadata: vec![
            Attribute::string(OBSERVATION_MODEL_NAME, request.model.clone()),
            Attribute::string(OBSERVATION_MODEL_PARAMETERS, parameters),
        ],
    })
}

pub fn finish_provider_call(observation: Observation, result: &Result<LlmCompletionResponse, LlmError>) {
    let outcome: ObservationOutcome = match result {
        Ok(response) => ObservationOutcome {
            status: ObservationStatus::Ok,
            output: observation.capture_content(&response),
            usage: response.usage.as_ref().map(generation_usage),
            ..ObservationOutcome::default()
        },
        Err(error) => ObservationOutcome {
            status: ObservationStatus::Error,
            error: Some(ObservationError {
                code: "llm_provider_call_failed".into(),
                failure_kind: "llm".into(),
                stage: Some("llm_provider_call".into()),
                message: error.to_string(),
            }),
            ..ObservationOutcome::default()
        },
    };
    observation.finish(outcome);
}

fn generation_usage(usage: &LlmTokenUsage) -> GenerationUsage {
    let input_cached_tokens = usage.cached_input_tokens.unwrap_or_default().min(usage.input_tokens);
    let output_reasoning_tokens = usage.reasoning_tokens.unwrap_or_default().min(usage.output_tokens);
    let input = usage.input_tokens.saturating_sub(input_cached_tokens);
    let output = usage.output_tokens.saturating_sub(output_reasoning_tokens);
    let total = input
        .saturating_add(input_cached_tokens)
        .saturating_add(output)
        .saturating_add(output_reasoning_tokens);
    GenerationUsage {
        input,
        input_cached_tokens,
        output,
        output_reasoning_tokens,
        total,
    }
}

#[cfg(test)]
#[path = "tests/llm_trace_tests.rs"]
mod tests;
