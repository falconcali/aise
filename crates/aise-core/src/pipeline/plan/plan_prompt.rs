use crate::core::{StoryContext, StoryPack};
use crate::pipeline::common::{PipelineError, PipelineStage, pipeline_prompt};
use crate::pipeline::plan::{PlanInput, plan_candidates::PlanCandidates};
use crate::prompt::{Prompt, PromptSpec, PromptVars, RenderedPrompt};
use crate::trace::Observation;
use serde_json::Value;

const PROCESS_STORY_PLAN_PROMPT_ID: &str = "plan.process_story_plan";
const STORY_SUMMARY_VAR: &str = "story_summary";
const STORY_OPENING_VAR: &str = "story_opening";
const RECENT_STORY_VAR: &str = "recent_story";
const KNOWN_FACTS_VAR: &str = "known_facts";
const RUMORS_VAR: &str = "rumors";
const RETRIEVABLE_FACTS_VAR: &str = "retrievable_facts";
const RETRIEVABLE_RUMORS_VAR: &str = "retrievable_rumors";
const NARRATIVE_DIRECTIONS_VAR: &str = "narrative_directions";
const STORY_CONSTRAINTS_VAR: &str = "story_constraints";
const PLAYER_INPUT_VAR: &str = "player_input";

pub fn process_plan(
    prompt: &Prompt,
    story_ctx: &StoryContext,
    input: &PlanInput<'_>,
    story_pack: &StoryPack,
    candidates: &PlanCandidates<'_>,
    observation: &Observation,
) -> Result<RenderedPrompt, PipelineError> {
    let story_opening = match pipeline_prompt::story_summary(story_ctx) {
        Some(_) => String::new(),
        None => story_pack.opening.as_str().to_owned(),
    };
    let vars = plan_vars(story_ctx, input, story_opening, candidates);
    prompt
        .render(PromptSpec::new(PROCESS_STORY_PLAN_PROMPT_ID, vars), observation)
        .map_err(PipelineError::prompt(PipelineStage::Plan))
}

fn plan_vars(
    story_ctx: &StoryContext,
    input: &PlanInput<'_>,
    story_opening: String,
    candidates: &PlanCandidates<'_>,
) -> PromptVars {
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
        (RETRIEVABLE_FACTS_VAR.to_owned(), Value::String(retrievable_facts(candidates))),
        (RETRIEVABLE_RUMORS_VAR.to_owned(), Value::String(retrievable_rumors(candidates))),
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

fn retrievable_facts(candidates: &PlanCandidates<'_>) -> String {
    candidates
        .facts
        .iter()
        .map(|candidate| candidate_line(candidate.id.as_str(), candidate.retrieval_hint.as_str()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn retrievable_rumors(candidates: &PlanCandidates<'_>) -> String {
    candidates
        .rumors
        .iter()
        .map(|candidate| candidate_line(candidate.id.as_str(), candidate.retrieval_hint.as_str()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn candidate_line(id: &str, retrieval_hint: &str) -> String {
    let id = serde_json::to_string(id).expect("string JSON serialization must succeed");
    let retrieval_hint = serde_json::to_string(retrieval_hint).expect("string JSON serialization must succeed");
    format!("- id: {id}\n  retrieval_hint: {retrieval_hint}")
}

#[cfg(test)]
#[path = "tests/plan_prompt_tests.rs"]
mod tests;
