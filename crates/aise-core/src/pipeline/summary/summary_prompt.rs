use crate::core::StoryContext;
use crate::pipeline::common::{PipelineError, PipelineStage, pipeline_prompt};
use crate::prompt::{Prompt, PromptSpec, PromptVars, RenderedPrompt};
use crate::trace::Observation;
use serde_json::Value;

const SUMMARIZE_STORY_PROMPT_ID: &str = "summary.summarize_story";
const STORY_SUMMARY_VAR: &str = "story_summary";
const STORY_OPENING_VAR: &str = "story_opening";
const STORY_TO_SUMMARIZE_VAR: &str = "story_to_summarize";

pub fn summarize_story(
    prompt: &Prompt,
    story_ctx: &StoryContext,
    covered_turns: usize,
    observation: &Observation,
) -> Result<RenderedPrompt, PipelineError> {
    let vars = summary_vars(story_ctx, covered_turns);
    prompt
        .render(PromptSpec::new(SUMMARIZE_STORY_PROMPT_ID, vars), observation)
        .map_err(PipelineError::prompt(PipelineStage::Summary))
}

fn summary_vars(story_ctx: &StoryContext, covered_turns: usize) -> PromptVars {
    PromptVars::from([
        (
            STORY_SUMMARY_VAR.to_owned(),
            Value::String(pipeline_prompt::story_summary(story_ctx).unwrap_or_default().to_owned()),
        ),
        (
            STORY_OPENING_VAR.to_owned(),
            Value::String(pipeline_prompt::story_opening(story_ctx).to_owned()),
        ),
        (
            STORY_TO_SUMMARIZE_VAR.to_owned(),
            Value::String(pipeline_prompt::turns_story(story_ctx.recent_turns.iter().take(covered_turns))),
        ),
    ])
}

#[cfg(test)]
#[path = "tests/summary_prompt_tests.rs"]
mod tests;
