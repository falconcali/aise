use crate::core::StoryContext;
use crate::persistence::StoryStore;
use crate::pipeline::common::{PipelineError, PipelineStage, pipeline_prompt};
use crate::prompt::{Prompt, PromptSpec, PromptVars, RenderedPrompt};
use crate::trace::Observation;
use serde_json::Value;

const PROCESS_PLAYER_INPUT_PROMPT_ID: &str = "baseline.process_player_input";
const PLAYER_INPUT_VAR: &str = "player_input";
const STORY_SUMMARY_VAR: &str = "story_summary";
const STORY_OPENING_VAR: &str = "story_opening";
const RECENT_STORY_VAR: &str = "recent_story";

pub async fn process_player_input(
    prompt: &Prompt,
    story_ctx: &StoryContext,
    input: &str,
    story_store: &dyn StoryStore,
    observation: &Observation,
) -> Result<RenderedPrompt, PipelineError> {
    let story_opening = pipeline_prompt::load_story_opening(PipelineStage::Baseline, story_ctx, story_store).await?;
    let vars = player_input_vars(story_ctx, input, story_opening);
    prompt
        .render(PromptSpec::new(PROCESS_PLAYER_INPUT_PROMPT_ID, vars), observation)
        .map_err(PipelineError::from)
}

fn player_input_vars(story_ctx: &StoryContext, input: &str, story_opening: String) -> PromptVars {
    PromptVars::from([
        (PLAYER_INPUT_VAR.to_owned(), Value::String(input.to_owned())),
        (
            STORY_SUMMARY_VAR.to_owned(),
            Value::String(pipeline_prompt::story_summary(story_ctx).unwrap_or_default().to_owned()),
        ),
        (STORY_OPENING_VAR.to_owned(), Value::String(story_opening)),
        (
            RECENT_STORY_VAR.to_owned(),
            Value::String(pipeline_prompt::recent_story(story_ctx)),
        ),
    ])
}

#[cfg(test)]
#[path = "tests/baseline_prompt_tests.rs"]
mod tests;
