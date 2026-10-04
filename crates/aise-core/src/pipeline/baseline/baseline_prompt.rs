use crate::prompt::{Prompt, PromptError, PromptSpec, PromptResult};
use crate::trace::Observation;

pub fn process_player_input(prompt: &Prompt, input: &str, observation: &Observation) -> Result<PromptResult, PromptError> {
    let spec = PromptSpec {
    };

    return prompt.render(spec, observation);
}