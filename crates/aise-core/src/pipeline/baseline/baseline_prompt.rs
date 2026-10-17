use crate::core::StoryContext;
use crate::pipeline::common::{PipelineError, PipelineStage, pipeline_prompt};
use crate::prompt::{Prompt, PromptSpec, PromptVars, RenderedPrompt};
use crate::trace::Observation;
use serde_json::Value;

const PROCESS_PLAYER_INPUT_PROMPT_ID: &str = "baseline.process_player_input";
const PLAYER_INPUT_VAR: &str = "player_input";
const STORY_SUMMARY_VAR: &str = "story_summary";
const STORY_OPENING_VAR: &str = "story_opening";
const RECENT_STORY_VAR: &str = "recent_story";

pub fn process_player_input(
    prompt: &Prompt,
    story_ctx: &StoryContext,
    input: &str,
    observation: &Observation,
) -> Result<RenderedPrompt, PipelineError> {
    let vars = player_input_vars(story_ctx, input);
    prompt
        .render(PromptSpec::new(PROCESS_PLAYER_INPUT_PROMPT_ID, vars), observation)
        .map_err(PipelineError::prompt(PipelineStage::Baseline))
}

fn player_input_vars(story_ctx: &StoryContext, input: &str) -> PromptVars {
    PromptVars::from([
        (PLAYER_INPUT_VAR.to_owned(), Value::String(input.to_owned())),
        (
            STORY_SUMMARY_VAR.to_owned(),
            Value::String(pipeline_prompt::story_summary(story_ctx).unwrap_or_default().to_owned()),
        ),
        (
            STORY_OPENING_VAR.to_owned(),
            Value::String(pipeline_prompt::story_opening(story_ctx).to_owned()),
        ),
        (
            RECENT_STORY_VAR.to_owned(),
            Value::String(pipeline_prompt::recent_story(story_ctx)),
        ),
    ])
}

#[cfg(test)]
#[path = "tests/baseline_prompt_tests.rs"]
mod tests;
