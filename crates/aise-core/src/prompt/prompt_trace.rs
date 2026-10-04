use crate::prompt::{PromptError, PromptSpec, RenderedPrompt};
use crate::trace::{
    Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus,
};

pub fn begin_prompt_render(observation: &Observation, spec: &PromptSpec) -> Observation {
    observation.begin(ObservationSpec {
        name: "prompt_render",
        kind: ObservationKind::Span,
        input: observation.capture_content(&spec),
        metadata: Vec::new(),
    })
}

pub fn finish_prompt_render(observation: Observation, result: &Result<RenderedPrompt, PromptError>) {
    let outcome = match result {
        Ok(rendered_prompt) => ObservationOutcome {
            status: ObservationStatus::Ok,
            output: observation.capture_content(&rendered_prompt),
            ..ObservationOutcome::default()
        },
        Err(error) => ObservationOutcome {
            status: ObservationStatus::Error,
            error: Some(ObservationError {
                code: "prompt_render_failed".into(),
                failure_kind: "prompt".into(),
                stage: Some("prompt_render".into()),
                message: error.to_string(),
            }),
            ..ObservationOutcome::default()
        },
    };
    observation.finish(outcome);
}
