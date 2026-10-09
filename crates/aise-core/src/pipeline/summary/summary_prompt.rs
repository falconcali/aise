use crate::core::StoryContext;
use crate::persistence::StoryStore;
use crate::pipeline::common::{PipelineError, PipelineStage, pipeline_prompt};
use crate::prompt::{Prompt, PromptSpec, PromptVars, RenderedPrompt};
use crate::trace::Observation;
use serde_json::Value;

const SUMMARIZE_STORY_PROMPT_ID: &str = "summary.summarize_story";
const STORY_SUMMARY_VAR: &str = "story_summary";
const STORY_OPENING_VAR: &str = "story_opening";
const STORY_TO_SUMMARIZE_VAR: &str = "story_to_summarize";

pub async fn summarize_story(
    prompt: &Prompt,
    story_ctx: &StoryContext,
    covered_turns: usize,
    story_store: &dyn StoryStore,
    observation: &Observation,
) -> Result<RenderedPrompt, PipelineError> {
    let story_opening = pipeline_prompt::load_story_opening(PipelineStage::Summary, story_ctx, story_store).await?;
    let vars = summary_vars(story_ctx, covered_turns, story_opening);
    prompt
        .render(PromptSpec::new(SUMMARIZE_STORY_PROMPT_ID, vars), observation)
        .map_err(|error| PipelineError::new(PipelineStage::Summary, error.to_string()))
}

fn summary_vars(story_ctx: &StoryContext, covered_turns: usize, story_opening: String) -> PromptVars {
    PromptVars::from([
        (
            STORY_SUMMARY_VAR.to_owned(),
            Value::String(pipeline_prompt::story_summary(story_ctx).unwrap_or_default().to_owned()),
        ),
        (STORY_OPENING_VAR.to_owned(), Value::String(story_opening)),
        (
            STORY_TO_SUMMARIZE_VAR.to_owned(),
            Value::String(pipeline_prompt::turns_story(story_ctx.recent_turns.iter().take(covered_turns))),
        ),
    ])
}

#[cfg(test)]
#[path = "tests/summary_prompt_tests.rs"]
mod tests;
