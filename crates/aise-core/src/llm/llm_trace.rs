use crate::llm::{LlmCompletionRequest, LlmCompletionResponse, LlmError};
use crate::trace::{
    Attribute, Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus,
};

pub fn begin_provider_call(observation: &Observation, request: &LlmCompletionRequest) -> Observation {
    observation.begin(ObservationSpec {
        name: "llm_provider_call",
        kind: ObservationKind::Generation,
        input: observation.capture_content(&request.messages),
        metadata: vec![
            Attribute::string("model", request.model.clone()),
            Attribute::string("temperature", request.temperature.to_string()),
        ],
    })
}

pub fn finish_provider_call(observation: Observation, result: &Result<LlmCompletionResponse, LlmError>) {
    let outcome: ObservationOutcome = match result {
        Ok(response) => ObservationOutcome {
            status: ObservationStatus::Ok,
            output: observation.capture_content(&response),
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
