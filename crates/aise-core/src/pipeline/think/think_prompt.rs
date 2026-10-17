use crate::core::{CharacterThinkRequest, ResolvedRole, StoryContext};
use crate::persistence::Store;
use crate::pipeline::common::{PipelineError, PipelineStage, pipeline_prompt};
use crate::pipeline::think::{ThinkConfig, ThinkInput, think_context};
use crate::prompt::{Prompt, PromptSpec, PromptVars, RenderedPrompt};
use crate::trace::Observation;
use serde_json::Value;

const CHARACTER_DECISION_PROMPT_ID: &str = "think.character_decision";
const STORY_SUMMARY_VAR: &str = "story_summary";
const STORY_OPENING_VAR: &str = "story_opening";
const RECENT_STORY_VAR: &str = "recent_story";
const PLAYER_CONTRIBUTION_VAR: &str = "player_contribution";
const RUMORS_VAR: &str = "rumors";
const TARGET_ROLE_ID_VAR: &str = "target_role_id";
const TARGET_NAME_VAR: &str = "target_name";
const TARGET_ALIASES_VAR: &str = "target_aliases";
const TARGET_PROFILE_VAR: &str = "target_profile";
const TARGET_BACKSTORY_VAR: &str = "target_backstory";
const TARGET_SEED_MEMORIES_VAR: &str = "target_seed_memories";
const THINKING_FOCUS_VAR: &str = "thinking_focus";

pub(super) struct CharacterDecisionPrompt<'a, 'b> {
    pub story_ctx: &'a StoryContext,
    pub input: &'a ThinkInput<'b>,
    pub request: &'a CharacterThinkRequest,
    pub role: &'a ResolvedRole,
    pub config: &'a ThinkConfig,
}

pub async fn character_decision(
    prompt: &Prompt,
    store: &Store,
    spec: CharacterDecisionPrompt<'_, '_>,
    observation: &Observation,
) -> Result<RenderedPrompt, PipelineError> {
    let profile = serialize_profile(spec.role)?;
    validate_context(spec.role, &profile, spec.config)?;
    let story_opening = pipeline_prompt::load_story_opening(PipelineStage::Think, spec.story_ctx, store).await?;
    let vars = PromptVars::from([
        (
            STORY_SUMMARY_VAR.to_owned(),
            Value::String(pipeline_prompt::story_summary(spec.story_ctx).unwrap_or_default().to_owned()),
        ),
        (STORY_OPENING_VAR.to_owned(), Value::String(story_opening)),
        (
            RECENT_STORY_VAR.to_owned(),
            Value::String(pipeline_prompt::recent_story(spec.story_ctx)),
        ),
        (
            PLAYER_CONTRIBUTION_VAR.to_owned(),
            Value::String(spec.input.player_contribution.processed.clone()),
        ),
        (
            RUMORS_VAR.to_owned(),
            Value::String(think_context::rumors(
                spec.input.activated_rumors,
                spec.input.retrieved_rumors,
                spec.config,
            )),
        ),
        (
            TARGET_ROLE_ID_VAR.to_owned(),
            Value::String(spec.role.role_id.as_str().to_owned()),
        ),
        (TARGET_NAME_VAR.to_owned(), Value::String(spec.role.name.as_str().to_owned())),
        (TARGET_ALIASES_VAR.to_owned(), Value::String(serialize_aliases(spec.role)?)),
        (TARGET_PROFILE_VAR.to_owned(), Value::String(profile)),
        (
            TARGET_BACKSTORY_VAR.to_owned(),
            Value::String(spec.role.backstory.as_str().to_owned()),
        ),
        (
            TARGET_SEED_MEMORIES_VAR.to_owned(),
            Value::String(think_context::memories(spec.role, spec.config)),
        ),
        (
            THINKING_FOCUS_VAR.to_owned(),
            Value::String(spec.request.thinking_focus.as_str().to_owned()),
        ),
    ]);
    prompt
        .render(PromptSpec::new(CHARACTER_DECISION_PROMPT_ID, vars), observation)
        .map_err(PipelineError::prompt(PipelineStage::Think))
}

fn serialize_profile(role: &ResolvedRole) -> Result<String, PipelineError> {
    serde_json::to_string(&role.profile).map_err(|error| {
        PipelineError::inconsistent_data(PipelineStage::Think, "think_profile_serialization", error.to_string())
    })
}

fn serialize_aliases(role: &ResolvedRole) -> Result<String, PipelineError> {
    serde_json::to_string(&role.aliases).map_err(|error| {
        PipelineError::inconsistent_data(PipelineStage::Think, "think_aliases_serialization", error.to_string())
    })
}

fn validate_context(role: &ResolvedRole, profile: &str, config: &ThinkConfig) -> Result<(), PipelineError> {
    if profile.len() > config.max_profile_bytes {
        return Err(PipelineError::inconsistent_data(
            PipelineStage::Think,
            "think_profile_too_long",
            format!(
                "serialized profile size {} exceeds maximum {}",
                profile.len(),
                config.max_profile_bytes
            ),
        ));
    }
    if role.backstory.as_str().len() > config.max_backstory_bytes {
        return Err(PipelineError::inconsistent_data(
            PipelineStage::Think,
            "think_backstory_too_long",
            format!(
                "backstory size {} exceeds maximum {}",
                role.backstory.as_str().len(),
                config.max_backstory_bytes
            ),
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/think_prompt_tests.rs"]
mod tests;
