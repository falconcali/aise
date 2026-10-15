use crate::core::{BoundedText, CharacterThinkRequest, FactId, PlayerRoleContext, RoleId, RumorId};
use crate::pipeline::common::{PipelineError, PipelineStage};
use crate::pipeline::plan::{
    PlanConfig, PlanOutput,
    plan_candidates::PlanCandidates,
    plan_roles::{self, RoleCatalogEntry},
};
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CharacterThinkRequestDto {
    role_id: RoleId,
    thinking_focus: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanLlmOutputDto {
    story_goal: String,
    facts_to_retrieve: Vec<FactId>,
    rumors_to_retrieve: Vec<RumorId>,
    roles_for_generation: Vec<RoleId>,
    character_think_requests: Vec<CharacterThinkRequestDto>,
}

pub(super) fn parse_plan_output(
    raw: &str,
    candidates: &PlanCandidates<'_>,
    role_catalog: &[RoleCatalogEntry<'_>],
    player_role: PlayerRoleContext,
    config: &PlanConfig,
) -> Result<PlanOutput, PipelineError> {
    let output = serde_json::from_str::<PlanLlmOutputDto>(raw).map_err(|error| {
        PipelineError::invalid_output(
            PipelineStage::Plan,
            "plan_output_invalid_json",
            format!("invalid plan JSON at line {} column {}", error.line(), error.column()),
        )
    })?;
    if output.story_goal.trim().is_empty() {
        return Err(PipelineError::invalid_output(
            PipelineStage::Plan,
            "plan_story_goal_empty",
            "story goal must not be empty",
        ));
    }
    if output.story_goal.len() > config.max_story_goal_bytes {
        return Err(PipelineError::invalid_output(
            PipelineStage::Plan,
            "plan_story_goal_too_long",
            "story goal exceeds the configured byte limit",
        ));
    }
    validate_fact_ids(&output.facts_to_retrieve, candidates)?;
    validate_rumor_ids(&output.rumors_to_retrieve, candidates)?;
    validate_generation_roles(&output.roles_for_generation, role_catalog, &player_role.role_id, config)?;
    let character_think_requests =
        validate_character_think_requests(output.character_think_requests, role_catalog, &player_role.role_id, config)?;
    let story_goal =
        BoundedText::try_new(output.story_goal, "story_goal", config.max_story_goal_bytes).map_err(|error| {
            PipelineError::invalid_output(PipelineStage::Plan, "plan_story_goal_too_long", error.to_string())
        })?;

    Ok(PlanOutput {
        story_goal,
        player_role,
        facts_to_retrieve: output.facts_to_retrieve,
        rumors_to_retrieve: output.rumors_to_retrieve,
        roles_for_generation: output.roles_for_generation,
        character_think_requests,
    })
}

fn validate_fact_ids(ids: &[FactId], candidates: &PlanCandidates<'_>) -> Result<(), PipelineError> {
    let mut seen = HashSet::with_capacity(ids.len().min(candidates.facts.len()));
    for id in ids {
        if !seen.insert(id) {
            return Err(PipelineError::invalid_output(
                PipelineStage::Plan,
                "plan_duplicate_fact_id",
                "duplicate fact ID selected",
            ));
        }
        if !candidates.contains_fact(id) {
            return Err(PipelineError::invalid_output(
                PipelineStage::Plan,
                "plan_unknown_fact_id",
                "selected fact ID was not offered",
            ));
        }
    }
    Ok(())
}

fn validate_rumor_ids(ids: &[RumorId], candidates: &PlanCandidates<'_>) -> Result<(), PipelineError> {
    let mut seen = HashSet::with_capacity(ids.len().min(candidates.rumors.len()));
    for id in ids {
        if !seen.insert(id) {
            return Err(PipelineError::invalid_output(
                PipelineStage::Plan,
                "plan_duplicate_rumor_id",
                "duplicate rumor ID selected",
            ));
        }
        if !candidates.contains_rumor(id) {
            return Err(PipelineError::invalid_output(
                PipelineStage::Plan,
                "plan_unknown_rumor_id",
                "selected rumor ID was not offered",
            ));
        }
    }
    Ok(())
}

fn validate_generation_roles(
    role_ids: &[RoleId],
    role_catalog: &[RoleCatalogEntry<'_>],
    player_role: &RoleId,
    config: &PlanConfig,
) -> Result<(), PipelineError> {
    if role_ids.len() > config.max_roles_for_generation {
        return Err(PipelineError::invalid_output(
            PipelineStage::Plan,
            "plan_too_many_generation_roles",
            "generation role selection exceeds the configured limit",
        ));
    }
    let mut seen = HashSet::with_capacity(role_ids.len().min(role_catalog.len()));
    for role_id in role_ids {
        if !seen.insert(role_id) {
            return Err(PipelineError::invalid_output(
                PipelineStage::Plan,
                "plan_duplicate_generation_role_id",
                "duplicate generation role ID selected",
            ));
        }
        if role_id == player_role {
            return Err(PipelineError::invalid_output(
                PipelineStage::Plan,
                "plan_player_generation_role_forbidden",
                "player role cannot be selected for generation",
            ));
        }
        if !plan_roles::contains_role(role_catalog, role_id) {
            return Err(PipelineError::invalid_output(
                PipelineStage::Plan,
                "plan_unknown_generation_role_id",
                "selected generation role ID was not offered",
            ));
        }
    }
    Ok(())
}

fn validate_character_think_requests(
    requests: Vec<CharacterThinkRequestDto>,
    role_catalog: &[RoleCatalogEntry<'_>],
    player_role: &RoleId,
    config: &PlanConfig,
) -> Result<Vec<CharacterThinkRequest>, PipelineError> {
    if requests.len() > config.max_character_think_requests {
        return Err(PipelineError::invalid_output(
            PipelineStage::Plan,
            "plan_too_many_character_think_requests",
            "character think requests exceed the configured limit",
        ));
    }
    let mut seen = HashSet::with_capacity(requests.len().min(role_catalog.len()));
    let mut validated = Vec::with_capacity(requests.len());
    for request in requests {
        if !seen.insert(request.role_id.clone()) {
            return Err(PipelineError::invalid_output(
                PipelineStage::Plan,
                "plan_duplicate_character_think_role_id",
                "duplicate character think role ID selected",
            ));
        }
        if &request.role_id == player_role {
            return Err(PipelineError::invalid_output(
                PipelineStage::Plan,
                "plan_player_character_think_forbidden",
                "player role cannot be a character think target",
            ));
        }
        if !plan_roles::contains_role(role_catalog, &request.role_id) {
            return Err(PipelineError::invalid_output(
                PipelineStage::Plan,
                "plan_unknown_character_think_role_id",
                "selected character think role ID was not offered",
            ));
        }
        let thinking_focus = request.thinking_focus.trim();
        if thinking_focus.is_empty() {
            return Err(PipelineError::invalid_output(
                PipelineStage::Plan,
                "plan_thinking_focus_empty",
                "thinking focus must not be empty",
            ));
        }
        if thinking_focus.len() > config.max_thinking_focus_bytes {
            return Err(PipelineError::invalid_output(
                PipelineStage::Plan,
                "plan_thinking_focus_too_long",
                "thinking focus exceeds the configured byte limit",
            ));
        }
        let thinking_focus = BoundedText::try_new(thinking_focus, "thinking_focus", config.max_thinking_focus_bytes)
            .map_err(|error| {
                PipelineError::invalid_output(PipelineStage::Plan, "plan_thinking_focus_too_long", error.to_string())
            })?;
        validated.push(CharacterThinkRequest {
            role_id: request.role_id,
            thinking_focus,
        });
    }
    Ok(validated)
}

#[cfg(test)]
#[path = "tests/plan_output_tests.rs"]
mod tests;
