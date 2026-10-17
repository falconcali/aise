use crate::core::{BoundedText, RoleEntry, StoryContext};
use crate::pipeline::common::{PipelineError, PipelineStage, pipeline_prompt};
use crate::pipeline::plan::{PlanConfig, PlanInput, plan_candidates::PlanCandidates, plan_roles::RoleCatalogEntry};
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
const PLAYER_ROLE_VAR: &str = "player_role";
const ROLE_CATALOG_VAR: &str = "role_catalog";
const NARRATIVE_DIRECTIONS_VAR: &str = "narrative_directions";
const STORY_CONSTRAINTS_VAR: &str = "story_constraints";
const PLAYER_INPUT_VAR: &str = "player_input";
const MAX_STORY_GOAL_BYTES_VAR: &str = "max_story_goal_bytes";

pub(super) struct PlanPromptContext<'a, 'b, 'c> {
    pub story_ctx: &'a StoryContext,
    pub input: &'a PlanInput<'b>,
    pub candidates: &'a PlanCandidates<'c>,
    pub player_role: &'a RoleEntry,
    pub role_catalog: &'a [RoleCatalogEntry<'c>],
    pub config: &'a PlanConfig,
}

pub(super) fn process_plan(
    prompt: &Prompt,
    context: PlanPromptContext<'_, '_, '_>,
    observation: &Observation,
) -> Result<RenderedPrompt, PipelineError> {
    let story_opening = pipeline_prompt::story_opening(context.story_ctx).to_owned();
    let player_role = player_role_text(context.player_role, context.config)?;
    let role_catalog = role_catalog_text(context.role_catalog)?;
    let vars = plan_vars(
        context.story_ctx,
        context.input,
        story_opening,
        context.candidates,
        player_role,
        role_catalog,
        context.config.max_story_goal_bytes,
    )?;
    prompt
        .render(PromptSpec::new(PROCESS_STORY_PLAN_PROMPT_ID, vars), observation)
        .map_err(PipelineError::prompt(PipelineStage::Plan))
}

fn plan_vars(
    story_ctx: &StoryContext,
    input: &PlanInput<'_>,
    story_opening: String,
    candidates: &PlanCandidates<'_>,
    player_role: String,
    role_catalog: String,
    max_story_goal_bytes: usize,
) -> Result<PromptVars, PipelineError> {
    Ok(PromptVars::from([
        (
            STORY_SUMMARY_VAR.to_owned(),
            Value::String(pipeline_prompt::story_summary(story_ctx).unwrap_or_default().to_owned()),
        ),
        (STORY_OPENING_VAR.to_owned(), Value::String(story_opening)),
        (
            RECENT_STORY_VAR.to_owned(),
            Value::String(pipeline_prompt::recent_story(story_ctx)),
        ),
        (PLAYER_ROLE_VAR.to_owned(), Value::String(player_role)),
        (
            KNOWN_FACTS_VAR.to_owned(),
            Value::String(pipeline_prompt::known_facts(&input.knowledge.facts)),
        ),
        (
            RUMORS_VAR.to_owned(),
            Value::String(pipeline_prompt::rumors(&input.knowledge.rumors)),
        ),
        (RETRIEVABLE_FACTS_VAR.to_owned(), Value::String(retrievable_facts(candidates)?)),
        (
            RETRIEVABLE_RUMORS_VAR.to_owned(),
            Value::String(retrievable_rumors(candidates)?),
        ),
        (ROLE_CATALOG_VAR.to_owned(), Value::String(role_catalog)),
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
        (
            MAX_STORY_GOAL_BYTES_VAR.to_owned(),
            Value::String(max_story_goal_bytes.to_string()),
        ),
    ]))
}

fn retrievable_facts(candidates: &PlanCandidates<'_>) -> Result<String, PipelineError> {
    candidates
        .facts
        .iter()
        .map(|candidate| candidate_line(candidate.id.as_str(), candidate.retrieval_hint.as_str()))
        .collect::<Result<Vec<_>, _>>()
        .map(|entries| entries.join("\n"))
}

fn retrievable_rumors(candidates: &PlanCandidates<'_>) -> Result<String, PipelineError> {
    candidates
        .rumors
        .iter()
        .map(|candidate| candidate_line(candidate.id.as_str(), candidate.retrieval_hint.as_str()))
        .collect::<Result<Vec<_>, _>>()
        .map(|entries| entries.join("\n"))
}

fn candidate_line(id: &str, retrieval_hint: &str) -> Result<String, PipelineError> {
    let id = serialize_prompt_text(id, "plan_candidate_serialization")?;
    let retrieval_hint = serialize_prompt_text(retrieval_hint, "plan_candidate_serialization")?;
    Ok(format!("- id: {id}\n  retrieval_hint: {retrieval_hint}"))
}

fn player_role_text(role: &RoleEntry, config: &PlanConfig) -> Result<String, PipelineError> {
    let role_id = serialize_prompt_text(role.id.as_str(), "plan_player_role_serialization")?;
    let name = serialize_prompt_text(role.spec.name.as_str(), "plan_player_role_serialization")?;
    let aliases = role
        .spec
        .alias
        .iter()
        .map(|alias| serialize_prompt_text(alias.as_str(), "plan_player_role_serialization"))
        .collect::<Result<Vec<_>, _>>()?
        .join(", ");
    let appearance = optional_text(role.profile.appearance.as_ref())?;
    let personality = optional_text(role.profile.personality.as_ref())?;
    let backstory = serialize_prompt_text(role.spec.backstory.as_str(), "plan_player_role_serialization")?;
    let rendered = format!(
        "role_id: {role_id}\nname: {name}\naliases: [{aliases}]\nappearance: {appearance}\npersonality: {personality}\nbackstory: {backstory}"
    );
    if rendered.len() > config.max_player_role_context_bytes {
        return Err(PipelineError::inconsistent_data(
            PipelineStage::Plan,
            "plan_player_role_context_too_large",
            format!(
                "player role context size {} exceeds maximum {}",
                rendered.len(),
                config.max_player_role_context_bytes
            ),
        ));
    }
    Ok(rendered)
}

fn optional_text(value: Option<&BoundedText>) -> Result<String, PipelineError> {
    match value {
        Some(value) => serialize_prompt_text(value.as_str(), "plan_player_role_serialization"),
        None => Ok(Value::Null.to_string()),
    }
}

fn role_catalog_text(catalog: &[RoleCatalogEntry<'_>]) -> Result<String, PipelineError> {
    catalog
        .iter()
        .map(|entry| {
            let role_id = serialize_catalog_text(entry.role_id.as_str())?;
            let name = serialize_catalog_text(entry.name.as_str())?;
            let aliases = entry
                .aliases
                .iter()
                .map(|alias| serialize_catalog_text(alias.as_str()))
                .collect::<Result<Vec<_>, _>>()?
                .join(", ");
            let retrieval_hint = serialize_catalog_text(entry.retrieval_hint.as_str())?;
            Ok(format!(
                "- role_id: {role_id}\n  name: {name}\n  aliases: [{aliases}]\n  retrieval_hint: {retrieval_hint}"
            ))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|entries| entries.join("\n"))
}

fn serialize_catalog_text(value: &str) -> Result<String, PipelineError> {
    serialize_prompt_text(value, "plan_role_catalog_serialization")
}

fn serialize_prompt_text(value: &str, code: &'static str) -> Result<String, PipelineError> {
    serde_json::to_string(value)
        .map_err(|error| PipelineError::inconsistent_data(PipelineStage::Plan, code, error.to_string()))
}

#[cfg(test)]
#[path = "tests/plan_prompt_tests.rs"]
mod tests;
