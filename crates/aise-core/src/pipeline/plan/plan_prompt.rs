use crate::core::{PlayerContribution, StoryContext};
use crate::persistence::StoryStore;
use crate::pipeline::common::{PipelineError, PipelineStage};
use crate::prompt::{Prompt, PromptSpec, PromptVars, RenderedPrompt};
use crate::trace::Observation;
use serde_json::Value;

const PROCESS_STORY_PLAN_PROMPT_ID: &str = "plan.process_story_plan";
const STORY_OPENING_VAR: &str = "story_opening";
const PLAYER_INPUT_VAR: &str = "player_input";

pub async fn process_plan(
    prompt: &Prompt,
    story_ctx: &StoryContext,
    player_contribution: &PlayerContribution,
    story_store: &dyn StoryStore,
    observation: &Observation,
) -> Result<RenderedPrompt, PipelineError> {
    let story_pack = story_store
        .get_pack(&story_ctx.pack_ref.pack_id)
        .await
        .map_err(|error| PipelineError::new(PipelineStage::Plan, error.to_string()))?;
    let vars = PromptVars::from([
        (STORY_OPENING_VAR.to_owned(), Value::String(story_pack.Opening)),
        (
            PLAYER_INPUT_VAR.to_owned(),
            Value::String(player_contribution.processed.clone()),
        ),
    ]);
    prompt
        .render(PromptSpec::new(PROCESS_STORY_PLAN_PROMPT_ID, vars), observation)
        .map_err(|error| PipelineError::new(PipelineStage::Plan, error.to_string()))
}
