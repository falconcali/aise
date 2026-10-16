use crate::core::StoryContext;
use crate::persistence::Store;
use crate::pipeline::common::{PipelineError, PipelineStage, ValidationContext, pipeline_prompt};
use serde::Serialize;

pub(in crate::pipeline) struct ValidationPromptEvidence {
    pub story_summary: String,
    pub story_opening: String,
    pub recent_story: String,
    pub story_goal: String,
    pub player_contribution: String,
    pub narrative_directions: String,
    pub story_constraints: String,
    pub activated_facts: String,
    pub activated_rumors: String,
    pub retrieved_facts: String,
    pub retrieved_rumors: String,
    pub player_role: String,
    pub resolved_roles: String,
    pub character_decisions: String,
}

pub(in crate::pipeline) async fn project_validation_evidence(
    stage: PipelineStage,
    story_ctx: &StoryContext,
    context: &ValidationContext<'_>,
    store: &Store,
) -> Result<ValidationPromptEvidence, PipelineError> {
    Ok(ValidationPromptEvidence {
        story_summary: pipeline_prompt::story_summary(story_ctx).unwrap_or_default().to_owned(),
        story_opening: pipeline_prompt::load_story_opening(stage, story_ctx, store).await?,
        recent_story: pipeline_prompt::recent_story(story_ctx),
        story_goal: context.story_goal.as_str().to_owned(),
        player_contribution: context.player_contribution.processed.clone(),
        narrative_directions: pipeline_prompt::narrative_directions(context.directions),
        story_constraints: pipeline_prompt::story_constraints(context.constraints),
        activated_facts: pipeline_prompt::known_facts(&context.knowledge.facts),
        activated_rumors: pipeline_prompt::rumors(&context.knowledge.rumors),
        retrieved_facts: pipeline_prompt::known_facts(context.retrieved_facts),
        retrieved_rumors: pipeline_prompt::rumors(context.retrieved_rumors),
        player_role: serialize(stage, &context.player_role, "validation_player_role_serialization")?,
        resolved_roles: serialize_optional_slice(stage, context.roles, "validation_resolved_roles_serialization")?,
        character_decisions: serialize_optional_slice(
            stage,
            context.character_decisions,
            "validation_character_decisions_serialization",
        )?,
    })
}

fn serialize<T: Serialize + ?Sized>(
    stage: PipelineStage,
    value: &T,
    code: &'static str,
) -> Result<String, PipelineError> {
    serde_json::to_string(value).map_err(|error| PipelineError::inconsistent_data(stage, code, error.to_string()))
}

fn serialize_optional_slice<T: Serialize>(
    stage: PipelineStage,
    value: &[T],
    code: &'static str,
) -> Result<String, PipelineError> {
    if value.is_empty() {
        return Ok(String::new());
    }
    serialize(stage, value, code)
}

#[cfg(test)]
#[path = "tests/validation_prompt_tests.rs"]
mod tests;
