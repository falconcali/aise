use crate::core::{StoryContext, TurnStatus};
use crate::persistence::StoryStore;
use crate::pipeline::common::{PipelineError, PipelineStage};
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
    let story_opening = load_story_opening(story_ctx, story_store).await?;
    let vars = player_input_vars(story_ctx, input, story_opening);
    prompt
        .render(PromptSpec::new(PROCESS_PLAYER_INPUT_PROMPT_ID, vars), observation)
        .map_err(PipelineError::from)
}

async fn load_story_opening(story_ctx: &StoryContext, story_store: &dyn StoryStore) -> Result<String, PipelineError> {
    if story_summary(story_ctx).is_some() {
        return Ok(String::new());
    }
    let story_pack = story_store
        .get_pack(&story_ctx.pack_ref.pack_id)
        .await
        .map_err(|error| PipelineError::new(PipelineStage::Baseline, error.to_string()))?;
    Ok(story_pack.Opening)
}

fn player_input_vars(story_ctx: &StoryContext, input: &str, story_opening: String) -> PromptVars {
    PromptVars::from([
        (PLAYER_INPUT_VAR.to_owned(), Value::String(input.to_owned())),
        (
            STORY_SUMMARY_VAR.to_owned(),
            Value::String(story_summary(story_ctx).unwrap_or_default().to_owned()),
        ),
        (STORY_OPENING_VAR.to_owned(), Value::String(story_opening)),
        (RECENT_STORY_VAR.to_owned(), Value::String(recent_story(story_ctx))),
    ])
}

fn story_summary(story_ctx: &StoryContext) -> Option<&str> {
    story_ctx
        .summary
        .as_ref()
        .map(|summary| summary.text.trim())
        .filter(|text| !text.is_empty())
}

fn recent_story(story_ctx: &StoryContext) -> String {
    story_ctx
        .rencent_turns
        .iter()
        .filter(|turn| matches!(turn.turn_status, TurnStatus::Accepted))
        .map(|turn| turn.turn_segment.text().trim())
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
#[path = "tests/baseline_prompt_tests.rs"]
mod tests;
