use crate::core::StoryContext;
use crate::persistence::StoryStore;
use crate::pipeline::common::{PipelineError, PipelineStage, pipeline_prompt};
use crate::pipeline::generate::GenerateInput;
use crate::prompt::{Prompt, PromptSpec, PromptVars, RenderedPrompt};
use crate::trace::Observation;
use serde_json::Value;

const GENERATE_STORY_PROMPT_ID: &str = "generate.generate_story";
const STORY_SUMMARY_VAR: &str = "story_summary";
const STORY_OPENING_VAR: &str = "story_opening";
const RECENT_STORY_VAR: &str = "recent_story";
const STORY_GOAL_VAR: &str = "story_goal";
const PLAYER_CONTRIBUTION_VAR: &str = "player_contribution";

pub async fn generate_story(
    prompt: &Prompt,
    story_ctx: &StoryContext,
    input: &GenerateInput,
    story_store: &dyn StoryStore,
    observation: &Observation,
) -> Result<RenderedPrompt, PipelineError> {
    let story_opening = pipeline_prompt::load_story_opening(PipelineStage::Generate, story_ctx, story_store).await?;
    let vars = generate_vars(story_ctx, input, story_opening);
    prompt
        .render(PromptSpec::new(GENERATE_STORY_PROMPT_ID, vars), observation)
        .map_err(|error| PipelineError::new(PipelineStage::Generate, error.to_string()))
}

fn generate_vars(story_ctx: &StoryContext, input: &GenerateInput, story_opening: String) -> PromptVars {
    PromptVars::from([
        (
            STORY_SUMMARY_VAR.to_owned(),
            Value::String(pipeline_prompt::story_summary(story_ctx).unwrap_or_default().to_owned()),
        ),
        (STORY_OPENING_VAR.to_owned(), Value::String(story_opening)),
        (
            RECENT_STORY_VAR.to_owned(),
            Value::String(pipeline_prompt::recent_story(story_ctx)),
        ),
        (STORY_GOAL_VAR.to_owned(), Value::String(input.story_goal.clone())),
        (
            PLAYER_CONTRIBUTION_VAR.to_owned(),
            Value::String(input.player_contribution.processed.clone()),
        ),
    ])
}

#[cfg(test)]
#[path = "tests/generate_prompt_tests.rs"]
mod tests;
