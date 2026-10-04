use crate::prompt::{Prompt, PromptError, PromptSpec, PromptVars, RenderedPrompt};
use serde_json::Value;

const PROCESS_PLAYER_INPUT_PROMPT_ID: &str = "baseline.process_player_input";
const PLAYER_INPUT_VAR: &str = "player_input";

pub fn process_player_input(prompt: &Prompt, input: &str) -> Result<RenderedPrompt, PromptError> {
    let vars = PromptVars::from([(PLAYER_INPUT_VAR.to_owned(), Value::String(input.to_owned()))]);
    prompt.render(PromptSpec::new(PROCESS_PLAYER_INPUT_PROMPT_ID, vars))
}
