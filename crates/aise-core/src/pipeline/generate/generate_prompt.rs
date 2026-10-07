use crate::core::StoryContext;
use crate::persistence::StoryStore;
use crate::pipeline::common::{PipelineError, PipelineStage};
use crate::pipeline::generate::GenerateInput;
use crate::prompt::{Prompt, PromptSpec, PromptVars, RenderedPrompt};
use crate::trace::Observation;
use serde_json::Value;

const GENERATE_STORY_PROMPT_ID: &str = "generate.generate_story";
const STORY_OPENING_VAR: &str = "story_opening";
const STORY_GOAL_VAR: &str = "story_goal";
const PLAYER_CONTRIBUTION_VAR: &str = "player_contribution";

pub async fn generate_story(
    prompt: &Prompt,
    story_ctx: &StoryContext,
    input: &GenerateInput,
    story_store: &dyn StoryStore,
    observation: &Observation,
) -> Result<RenderedPrompt, PipelineError> {
    let story_pack = story_store
        .get_pack(&story_ctx.pack_ref.pack_id)
        .await
        .map_err(|error| PipelineError::new(PipelineStage::Generate, error.to_string()))?;
    let vars = PromptVars::from([
        (STORY_OPENING_VAR.to_owned(), Value::String(story_pack.Opening)),
        (STORY_GOAL_VAR.to_owned(), Value::String(input.story_goal.clone())),
        (
            PLAYER_CONTRIBUTION_VAR.to_owned(),
            Value::String(input.player_contribution.processed.clone()),
        ),
    ]);
    prompt
        .render(PromptSpec::new(GENERATE_STORY_PROMPT_ID, vars), observation)
        .map_err(|error| PipelineError::new(PipelineStage::Generate, error.to_string()))
}
