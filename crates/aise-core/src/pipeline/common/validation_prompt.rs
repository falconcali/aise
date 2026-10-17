use crate::core::{BoundedText, CharacterProfile, RoleEntry, RoleId, StoryContext};
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

#[derive(Serialize)]
struct RolePromptView<'a> {
    role_id: &'a RoleId,
    name: &'a BoundedText,
    aliases: &'a [BoundedText],
    profile: &'a CharacterProfile,
    backstory: &'a BoundedText,
}

pub(in crate::pipeline) fn project_validation_evidence(
    stage: PipelineStage,
    story_ctx: &StoryContext,
    context: &ValidationContext<'_>,
) -> Result<ValidationPromptEvidence, PipelineError> {
    let player_role = resolve_role(stage, story_ctx, &story_ctx.player)?;
    let roles = context
        .role_ids
        .iter()
        .map(|role_id| resolve_role(stage, story_ctx, role_id).map(role_view))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ValidationPromptEvidence {
        story_summary: pipeline_prompt::story_summary(story_ctx).unwrap_or_default().to_owned(),
        story_opening: pipeline_prompt::story_opening(story_ctx).to_owned(),
        recent_story: pipeline_prompt::recent_story(story_ctx),
        story_goal: context.story_goal.as_str().to_owned(),
        player_contribution: context.player_contribution.processed.clone(),
        narrative_directions: pipeline_prompt::narrative_directions(context.directions),
        story_constraints: pipeline_prompt::story_constraints(context.constraints),
        activated_facts: pipeline_prompt::known_facts(&context.knowledge.facts),
        activated_rumors: pipeline_prompt::rumors(&context.knowledge.rumors),
        retrieved_facts: pipeline_prompt::known_facts(context.retrieved_facts),
        retrieved_rumors: pipeline_prompt::rumors(context.retrieved_rumors),
        player_role: serialize(stage, &role_view(player_role), "validation_player_role_serialization")?,
        resolved_roles: serialize_optional_slice(stage, &roles, "validation_resolved_roles_serialization")?,
        character_decisions: serialize_optional_slice(
            stage,
            context.character_decisions,
            "validation_character_decisions_serialization",
        )?,
    })
}

fn resolve_role<'a>(
    stage: PipelineStage,
    story_ctx: &'a StoryContext,
    role_id: &RoleId,
) -> Result<&'a RoleEntry, PipelineError> {
    story_ctx.roles.get(role_id).ok_or_else(|| {
        PipelineError::inconsistent_data(
            stage,
            missing_role_code(stage),
            format!("role ID {role_id} is missing from the story context"),
        )
    })
}

fn missing_role_code(stage: PipelineStage) -> &'static str {
    match stage {
        PipelineStage::Validate => "validate_role_missing",
        PipelineStage::Repair => "repair_role_missing",
        _ => "validation_role_missing",
    }
}

fn role_view(role: &RoleEntry) -> RolePromptView<'_> {
    RolePromptView {
        role_id: &role.id,
        name: &role.spec.name,
        aliases: &role.spec.alias,
        profile: &role.profile,
        backstory: &role.spec.backstory,
    }
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
