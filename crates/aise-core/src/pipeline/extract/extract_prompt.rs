use crate::core::{SemanticQuery, StoryContext, StoryPack};
use crate::pipeline::common::{PipelineError, PipelineStage, pipeline_prompt};
use crate::prompt::{Prompt, PromptSpec, PromptVars, RenderedPrompt};
use crate::trace::Observation;
use serde_json::Value;

const EVALUATE_NARRATIVE_PROMPT_ID: &str = "extract.evaluate_narrative";
const STORY_SUMMARY_VAR: &str = "story_summary";
const STORY_OPENING_VAR: &str = "story_opening";
const RECENT_STORY_VAR: &str = "recent_story";
const CURRENT_PROPOSAL_VAR: &str = "current_proposal";
const SEMANTIC_QUERIES_VAR: &str = "semantic_queries";

pub(super) struct EvaluateNarrativePrompt<'a> {
    pub story_pack: &'a StoryPack,
    pub story_ctx: &'a StoryContext,
    pub proposal: &'a str,
    pub queries: &'a [SemanticQuery],
}

pub(super) fn evaluate_narrative(
    prompt: &Prompt,
    spec: EvaluateNarrativePrompt<'_>,
    observation: &Observation,
) -> Result<RenderedPrompt, PipelineError> {
    let story_summary = pipeline_prompt::story_summary(spec.story_ctx).unwrap_or_default();
    let story_opening = if story_summary.is_empty() {
        spec.story_pack.opening.as_str()
    } else {
        ""
    };
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

#[cfg(test)]
#[path = "tests/extract_prompt_tests.rs"]
mod tests;
