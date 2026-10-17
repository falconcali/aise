use crate::core::{RoleId, SemanticQuery, StoryContext};
use crate::pipeline::common::{PipelineError, PipelineStage, pipeline_prompt};
use crate::pipeline::extract::{ExtractConfig, extract_knowledge};
use crate::prompt::{Prompt, PromptSpec, PromptVars, RenderedPrompt};
use crate::trace::Observation;
use serde_json::Value;

const EVALUATE_NARRATIVE_PROMPT_ID: &str = "extract.evaluate_narrative";
const EXTRACT_WORLD_KNOWLEDGE_PROMPT_ID: &str = "extract.extract_world_knowledge";
const EXTRACT_ROLE_MEMORY_PROMPT_ID: &str = "extract.extract_role_memory";
const STORY_SUMMARY_VAR: &str = "story_summary";
const STORY_OPENING_VAR: &str = "story_opening";
const RECENT_STORY_VAR: &str = "recent_story";
const CURRENT_PROPOSAL_VAR: &str = "current_proposal";
const SEMANTIC_QUERIES_VAR: &str = "semantic_queries";
const EXISTING_FACTS_VAR: &str = "existing_facts";
const EXISTING_RUMORS_VAR: &str = "existing_rumors";
const ROLE_CATALOG_VAR: &str = "role_catalog";
const MAX_FACT_CHANGES_VAR: &str = "max_fact_changes";
const MAX_RUMOR_CHANGES_VAR: &str = "max_rumor_changes";
const MAX_ROLES_FOR_MEMORY_VAR: &str = "max_roles_for_memory";
const TARGET_ROLE_ID_VAR: &str = "target_role_id";
const TARGET_NAME_VAR: &str = "target_name";
const TARGET_ALIASES_VAR: &str = "target_aliases";
const TARGET_BACKSTORY_VAR: &str = "target_backstory";
const EXISTING_MEMORIES_VAR: &str = "existing_memories";
const MAX_MEMORY_CHANGES_PER_ROLE_VAR: &str = "max_memory_changes_per_role";

pub(super) struct EvaluateNarrativePrompt<'a> {
    pub story_ctx: &'a StoryContext,
    pub proposal: &'a str,
    pub queries: &'a [SemanticQuery],
}

pub(super) struct ExtractWorldKnowledgePrompt<'a> {
    pub story_ctx: &'a StoryContext,
    pub proposal: &'a str,
    pub config: &'a ExtractConfig,
}

pub(super) struct ExtractRoleMemoryPrompt<'a> {
    pub story_ctx: &'a StoryContext,
    pub proposal: &'a str,
    pub role_id: &'a RoleId,
    pub config: &'a ExtractConfig,
}

pub(super) fn evaluate_narrative(
    prompt: &Prompt,
    spec: EvaluateNarrativePrompt<'_>,
    observation: &Observation,
) -> Result<RenderedPrompt, PipelineError> {
    let (story_summary, story_opening) = story_context_vars(spec.story_ctx);
    let semantic_queries = serde_json::to_string(spec.queries).map_err(|error| {
        PipelineError::inconsistent_data(
            PipelineStage::Extract,
            "extract_narrative_query_serialization",
            error.to_string(),
        )
    })?;
    let vars = PromptVars::from([
        (STORY_SUMMARY_VAR.to_owned(), Value::String(story_summary.to_owned())),
        (STORY_OPENING_VAR.to_owned(), Value::String(story_opening.to_owned())),
        (
            RECENT_STORY_VAR.to_owned(),
            Value::String(pipeline_prompt::recent_story(spec.story_ctx)),
        ),
        (CURRENT_PROPOSAL_VAR.to_owned(), Value::String(spec.proposal.to_owned())),
        (SEMANTIC_QUERIES_VAR.to_owned(), Value::String(semantic_queries)),
    ]);
    prompt
        .render(PromptSpec::new(EVALUATE_NARRATIVE_PROMPT_ID, vars), observation)
        .map_err(PipelineError::prompt(PipelineStage::Extract))
}

pub(super) fn extract_world_knowledge(
    prompt: &Prompt,
    spec: ExtractWorldKnowledgePrompt<'_>,
    observation: &Observation,
) -> Result<RenderedPrompt, PipelineError> {
    let (story_summary, story_opening) = story_context_vars(spec.story_ctx);
    let existing_facts = serde_json::to_string(&extract_knowledge::project_facts(spec.story_ctx)).map_err(|error| {
        PipelineError::inconsistent_data(PipelineStage::Extract, "extract_world_facts_serialization", error.to_string())
    })?;
    let existing_rumors =
        serde_json::to_string(&extract_knowledge::project_rumors(spec.story_ctx)).map_err(|error| {
            PipelineError::inconsistent_data(
                PipelineStage::Extract,
                "extract_world_rumors_serialization",
                error.to_string(),
            )
        })?;
    let role_catalog =
        serde_json::to_string(&extract_knowledge::project_role_catalog(spec.story_ctx)).map_err(|error| {
            PipelineError::inconsistent_data(
                PipelineStage::Extract,
                "extract_role_catalog_serialization",
                error.to_string(),
            )
        })?;
    let vars = PromptVars::from([
        (STORY_SUMMARY_VAR.to_owned(), Value::String(story_summary.to_owned())),
        (STORY_OPENING_VAR.to_owned(), Value::String(story_opening.to_owned())),
        (
            RECENT_STORY_VAR.to_owned(),
            Value::String(pipeline_prompt::recent_story(spec.story_ctx)),
        ),
        (CURRENT_PROPOSAL_VAR.to_owned(), Value::String(spec.proposal.to_owned())),
        (EXISTING_FACTS_VAR.to_owned(), Value::String(existing_facts)),
        (EXISTING_RUMORS_VAR.to_owned(), Value::String(existing_rumors)),
        (ROLE_CATALOG_VAR.to_owned(), Value::String(role_catalog)),
        (MAX_FACT_CHANGES_VAR.to_owned(), Value::from(spec.config.max_fact_changes)),
        (MAX_RUMOR_CHANGES_VAR.to_owned(), Value::from(spec.config.max_rumor_changes)),
        (
            MAX_ROLES_FOR_MEMORY_VAR.to_owned(),
            Value::from(spec.config.max_roles_for_memory),
        ),
    ]);
    prompt
        .render(PromptSpec::new(EXTRACT_WORLD_KNOWLEDGE_PROMPT_ID, vars), observation)
        .map_err(PipelineError::prompt(PipelineStage::Extract))
}

pub(super) fn extract_role_memory(
    prompt: &Prompt,
    spec: ExtractRoleMemoryPrompt<'_>,
    observation: &Observation,
) -> Result<RenderedPrompt, PipelineError> {
    let role = spec.story_ctx.roles.get(spec.role_id).ok_or_else(|| {
        PipelineError::invalid_output(
            PipelineStage::Extract,
            "extract_role_unknown",
            format!("role {} does not exist in story context", spec.role_id),
        )
    })?;
    let (story_summary, story_opening) = story_context_vars(spec.story_ctx);
    let target_aliases = serde_json::to_string(&role.spec.alias).map_err(|error| {
        PipelineError::inconsistent_data(
            PipelineStage::Extract,
            "extract_role_aliases_serialization",
            error.to_string(),
        )
    })?;
    let memories = extract_knowledge::project_role_memories(spec.story_ctx, spec.role_id)?;
    let existing_memories = serde_json::to_string(&memories).map_err(|error| {
        PipelineError::inconsistent_data(
            PipelineStage::Extract,
            "extract_role_memories_serialization",
            error.to_string(),
        )
    })?;
    let vars = PromptVars::from([
        (STORY_SUMMARY_VAR.to_owned(), Value::String(story_summary.to_owned())),
        (STORY_OPENING_VAR.to_owned(), Value::String(story_opening.to_owned())),
        (
            RECENT_STORY_VAR.to_owned(),
            Value::String(pipeline_prompt::recent_story(spec.story_ctx)),
        ),
        (CURRENT_PROPOSAL_VAR.to_owned(), Value::String(spec.proposal.to_owned())),
        (TARGET_ROLE_ID_VAR.to_owned(), Value::String(spec.role_id.as_str().to_owned())),
        (TARGET_NAME_VAR.to_owned(), Value::String(role.spec.name.as_str().to_owned())),
        (TARGET_ALIASES_VAR.to_owned(), Value::String(target_aliases)),
        (
            TARGET_BACKSTORY_VAR.to_owned(),
            Value::String(role.spec.backstory.as_str().to_owned()),
        ),
        (EXISTING_MEMORIES_VAR.to_owned(), Value::String(existing_memories)),
        (
            MAX_MEMORY_CHANGES_PER_ROLE_VAR.to_owned(),
            Value::from(spec.config.max_memory_changes_per_role),
        ),
    ]);
    prompt
        .render(PromptSpec::new(EXTRACT_ROLE_MEMORY_PROMPT_ID, vars), observation)
        .map_err(PipelineError::prompt(PipelineStage::Extract))
}

fn story_context_vars(story_ctx: &StoryContext) -> (&str, &str) {
    let story_summary = pipeline_prompt::story_summary(story_ctx).unwrap_or_default();
    let story_opening = pipeline_prompt::story_opening(story_ctx);
    (story_summary, story_opening)
}

#[cfg(test)]
#[path = "tests/extract_prompt_tests.rs"]
mod tests;
