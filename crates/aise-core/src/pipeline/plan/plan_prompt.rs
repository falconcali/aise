use crate::core::{PlayerContribution, StoryContext};
use crate::persistence::StoryStore;
use crate::pipeline::common::{PipelineError, PipelineStage, pipeline_prompt};
use crate::prompt::{Prompt, PromptSpec, PromptVars, RenderedPrompt};
use crate::trace::Observation;
use serde_json::Value;

const PROCESS_STORY_PLAN_PROMPT_ID: &str = "plan.process_story_plan";
const STORY_SUMMARY_VAR: &str = "story_summary";
const STORY_OPENING_VAR: &str = "story_opening";
const RECENT_STORY_VAR: &str = "recent_story";
const PLAYER_INPUT_VAR: &str = "player_input";

pub async fn process_plan(
    prompt: &Prompt,
    story_ctx: &StoryContext,
    player_contribution: &PlayerContribution,
    story_store: &dyn StoryStore,
    observation: &Observation,
) -> Result<RenderedPrompt, PipelineError> {
    let story_opening = pipeline_prompt::load_continuation_opening(PipelineStage::Plan, story_ctx, story_store).await?;
    let vars = plan_vars(story_ctx, player_contribution, story_opening);
    prompt
        .render(PromptSpec::new(PROCESS_STORY_PLAN_PROMPT_ID, vars), observation)
        .map_err(|error| PipelineError::new(PipelineStage::Plan, error.to_string()))
}

fn plan_vars(story_ctx: &StoryContext, player_contribution: &PlayerContribution, story_opening: String) -> PromptVars {
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
        (
            PLAYER_INPUT_VAR.to_owned(),
            Value::String(player_contribution.processed.clone()),
        ),
    ])
}

#[cfg(test)]
#[path = "tests/plan_prompt_tests.rs"]
mod tests;
