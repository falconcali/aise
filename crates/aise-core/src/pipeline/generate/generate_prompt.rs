use crate::core::StoryContext;
use crate::persistence::Store;
use crate::pipeline::common::{PipelineError, PipelineStage, pipeline_prompt};
use crate::pipeline::generate::GenerateInput;
use crate::prompt::{Prompt, PromptSpec, PromptVars, RenderedPrompt};
use crate::trace::Observation;
use serde_json::Value;

const GENERATE_STORY_PROMPT_ID: &str = "generate.generate_story";
const STORY_SUMMARY_VAR: &str = "story_summary";
const STORY_OPENING_VAR: &str = "story_opening";
const RECENT_STORY_VAR: &str = "recent_story";
const KNOWN_FACTS_VAR: &str = "known_facts";
const RUMORS_VAR: &str = "rumors";
const NARRATIVE_DIRECTIONS_VAR: &str = "narrative_directions";
const STORY_CONSTRAINTS_VAR: &str = "story_constraints";
const STORY_GOAL_VAR: &str = "story_goal";
const PLAYER_CONTRIBUTION_VAR: &str = "player_contribution";

pub async fn generate_story(
    prompt: &Prompt,
    story_ctx: &StoryContext,
    input: &GenerateInput<'_>,
    store: &Store,
    observation: &Observation,
) -> Result<RenderedPrompt, PipelineError> {
    let story_opening = pipeline_prompt::load_story_opening(PipelineStage::Generate, story_ctx, store).await?;
    let vars = generate_vars(story_ctx, input, story_opening);
    prompt
        .render(PromptSpec::new(GENERATE_STORY_PROMPT_ID, vars), observation)
        .map_err(PipelineError::prompt(PipelineStage::Generate))
}

fn generate_vars(story_ctx: &StoryContext, input: &GenerateInput<'_>, story_opening: String) -> PromptVars {
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
            KNOWN_FACTS_VAR.to_owned(),
            Value::String(pipeline_prompt::known_facts(&input.knowledge.facts)),
        ),
        (
            RUMORS_VAR.to_owned(),
            Value::String(pipeline_prompt::rumors(&input.knowledge.rumors)),
        ),
        (
            NARRATIVE_DIRECTIONS_VAR.to_owned(),
            Value::String(pipeline_prompt::narrative_directions(input.directions)),
        ),
        (
            STORY_CONSTRAINTS_VAR.to_owned(),
            Value::String(pipeline_prompt::story_constraints(input.constraints)),
        ),
        (STORY_GOAL_VAR.to_owned(), Value::String(input.story_goal.to_owned())),
        (
            PLAYER_CONTRIBUTION_VAR.to_owned(),
            Value::String(input.player_contribution.processed.clone()),
        ),
    ])
}

#[cfg(test)]
#[path = "tests/generate_prompt_tests.rs"]
mod tests;
