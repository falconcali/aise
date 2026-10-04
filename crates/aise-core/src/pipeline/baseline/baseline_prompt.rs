use crate::prompt::{Prompt, PromptError, PromptSpec, RenderedPrompt};

pub fn process_player_input(
    prompt: &Prompt,
    input: &str,
) -> Result<RenderedPrompt, PromptError> {
    let spec = PromptSpec {};

    return prompt.render(spec);
}