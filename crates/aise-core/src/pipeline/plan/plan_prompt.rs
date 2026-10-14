use crate::core::StoryContext;
use crate::persistence::Store;
use crate::pipeline::common::{PipelineError, PipelineStage, pipeline_prompt};
use crate::pipeline::plan::PlanInput;
use crate::prompt::{Prompt, PromptSpec, PromptVars, RenderedPrompt};
use crate::trace::Observation;
use serde_json::Value;

const PROCESS_STORY_PLAN_PROMPT_ID: &str = "plan.process_story_plan";
const STORY_SUMMARY_VAR: &str = "story_summary";
const STORY_OPENING_VAR: &str = "story_opening";
const RECENT_STORY_VAR: &str = "recent_story";
const KNOWN_FACTS_VAR: &str = "known_facts";
const RUMORS_VAR: &str = "rumors";
const NARRATIVE_DIRECTIONS_VAR: &str = "narrative_directions";
const STORY_CONSTRAINTS_VAR: &str = "story_constraints";
const PLAYER_INPUT_VAR: &str = "player_input";

pub async fn process_plan(
    prompt: &Prompt,
    story_ctx: &StoryContext,
    input: &PlanInput<'_>,
    store: &Store,
    observation: &Observation,
) -> Result<RenderedPrompt, PipelineError> {
    let story_opening = pipeline_prompt::load_story_opening(PipelineStage::Plan, story_ctx, store).await?;
    let vars = plan_vars(story_ctx, input, story_opening);
    prompt
        .render(PromptSpec::new(PROCESS_STORY_PLAN_PROMPT_ID, vars), observation)
        .map_err(PipelineError::prompt(PipelineStage::Plan))
}

fn plan_vars(story_ctx: &StoryContext, input: &PlanInput<'_>, story_opening: String) -> PromptVars {
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
        (
            PLAYER_INPUT_VAR.to_owned(),
            Value::String(input.player_contribution.processed.clone()),
        ),
    ])
}

#[cfg(test)]
#[path = "tests/plan_prompt_tests.rs"]
mod tests;
