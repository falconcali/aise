use crate::core::{BoundedText, CharacterDecision, CharacterProfile, PlayerRoleContext, ResolvedRole, RoleId};
use crate::pipeline::common::{PipelineError, PipelineStage};
use crate::pipeline::generate::GenerateConfig;
use serde_json::{Map, Value};
use std::collections::HashSet;

const TRUNCATION_MARKER: &str = "[truncated]";

#[derive(Debug)]
pub(super) struct GenerateRoleContext {
    pub player_role: String,
    pub roles: String,
    pub decisions: String,
}

pub(super) fn project_role_context(
    player_role: &PlayerRoleContext,
    roles: &[ResolvedRole],
    decisions: &[CharacterDecision],
    config: &GenerateConfig,
) -> Result<GenerateRoleContext, PipelineError> {
    validate_ai_roles(player_role, roles)?;
    validate_decisions(player_role, roles, decisions)?;
    let player_role = render_player_role(player_role, config)?;
    let roles = render_roles(roles, config)?;
    validate_aggregate_role_context(&player_role, &roles, config)?;
    Ok(GenerateRoleContext {
        player_role,
        roles,
        decisions: render_decisions(decisions, config)?,
    })
}

pub(super) fn validate_player_role(
    player_role: &PlayerRoleContext,
    expected_role_id: &RoleId,
) -> Result<(), PipelineError> {
    if &player_role.role_id != expected_role_id {
        return Err(inconsistent(
            "generate_player_role_mismatch",
            "Player Role Context does not match the story player role",
        ));
    }
    Ok(())
}

fn validate_ai_roles(player_role: &PlayerRoleContext, roles: &[ResolvedRole]) -> Result<(), PipelineError> {
    if roles.iter().any(|role| role.role_id == player_role.role_id) {
        return Err(inconsistent(
            "generate_player_role_in_ai_roles",
            "player role appears in the AI role collection",
        ));
    }
    Ok(())
}

fn validate_decisions(
    player_role: &PlayerRoleContext,
    roles: &[ResolvedRole],
    decisions: &[CharacterDecision],
) -> Result<(), PipelineError> {
    let mut seen = HashSet::with_capacity(decisions.len());
    for decision in decisions {
        if decision.role_id == player_role.role_id {
            return Err(inconsistent(
                "generate_player_character_decision_forbidden",
                "Character Decision targets the player role",
            ));
        }
        if !seen.insert(&decision.role_id) {
            return Err(inconsistent(
                "generate_duplicate_character_decision",
                "Character Decision role appears more than once",
            ));
        }
        if !roles.iter().any(|role| role.role_id == decision.role_id) {
            return Err(inconsistent(
                "generate_character_decision_role_missing",
                "Character Decision role is missing from resolved roles",
            ));
        }
    }
    Ok(())
}

fn validate_aggregate_role_context(
    player_role: &str,
    roles: &str,
    config: &GenerateConfig,
) -> Result<(), PipelineError> {
    let separator_bytes = usize::from(!roles.is_empty()).saturating_mul(2);
    if player_role.len().saturating_add(separator_bytes).saturating_add(roles.len()) > config.max_role_context_bytes {
        return Err(inconsistent(
            "generate_role_context_too_large",
            "resolved role context exceeds the aggregate byte budget",
        ));
    }
    Ok(())
}

fn render_roles(roles: &[ResolvedRole], config: &GenerateConfig) -> Result<String, PipelineError> {
    let mut rendered = String::new();
    for role in roles {
        let role = render_role(&role.role_id, &role.name, &role.aliases, &role.profile, &role.backstory, config)?;
        if !rendered.is_empty() {
            rendered.push_str("\n\n");
        }
        rendered.push_str(&role);
    }
    Ok(rendered)
}

fn render_player_role(role: &PlayerRoleContext, config: &GenerateConfig) -> Result<String, PipelineError> {
    render_role(&role.role_id, &role.name, &role.aliases, &role.profile, &role.backstory, config)
}

fn render_role(
    role_id: &RoleId,
    name: &BoundedText,
    aliases: &[BoundedText],
    profile: &CharacterProfile,
    backstory: &BoundedText,
    config: &GenerateConfig,
) -> Result<String, PipelineError> {
    let mut rendered = [
        format!("role_id: {}", json_string(role_id.as_str())),
        format!("name: {}", json_string(name.as_str())),
        format!("aliases: {}", aliases_json(aliases)),
        format!("profile.appearance: {}", optional_text_json(profile.appearance.as_ref())),
        format!("profile.personality: {}", optional_text_json(profile.personality.as_ref())),
        format!(
            "profile.speaking_style: {}",
            optional_text_json(profile.speaking_style.as_ref())
        ),
        format!("profile.dialogue_examples: {}", dialogue_examples_json(profile)),
    ]
    .join("\n");
    let backstory_prefix = "\nbackstory: ";
    if rendered.len() > config.max_role_bytes_per_role {
        return Err(inconsistent(
            "generate_role_projection_too_large",
            "mandatory role identity and profile exceed the per-role byte budget",
        ));
    }
    let full_backstory = json_string(backstory.as_str());
    let full_bytes = rendered
        .len()
        .saturating_add(backstory_prefix.len())
        .saturating_add(full_backstory.len());
    let backstory = if full_bytes <= config.max_role_bytes_per_role {
        full_backstory
    } else {
        let minimum_backstory = json_string(TRUNCATION_MARKER);
        let minimum_bytes = rendered
            .len()
            .saturating_add(backstory_prefix.len())
            .saturating_add(minimum_backstory.len());
        if minimum_bytes > config.max_role_bytes_per_role {
            return Err(inconsistent(
                "generate_role_projection_too_large",
                "truncated role backstory cannot fit the per-role byte budget",
            ));
        }
        let available = config
            .max_role_bytes_per_role
            .saturating_sub(rendered.len())
            .saturating_sub(backstory_prefix.len());
        truncated_json_string(backstory.as_str(), available).ok_or_else(|| {
            inconsistent(
                "generate_role_projection_too_large",
                "truncated role backstory cannot fit the per-role byte budget",
            )
        })?
    };
    rendered.push_str(backstory_prefix);
    rendered.push_str(&backstory);
    Ok(rendered)
}

fn render_decisions(decisions: &[CharacterDecision], config: &GenerateConfig) -> Result<String, PipelineError> {
    let mut rendered = String::new();
    for decision in decisions {
        let utterance = decision
            .suggested_utterance
            .as_ref()
            .map_or(Value::Null, |value| Value::String(value.as_str().to_owned()));
        let item = format!(
            "- role_id: {}\n  decision: {}\n  suggested_utterance: {}",
            json_string(decision.role_id.as_str()),
            json_string(decision.decision.as_str()),
            utterance
        );
        let separator_bytes = usize::from(!rendered.is_empty());
        if rendered.len().saturating_add(separator_bytes).saturating_add(item.len()) > config.max_decision_context_bytes
        {
            return Err(inconsistent(
                "generate_decision_context_too_large",
                "Character Decision context exceeds the byte budget",
            ));
        }
        if !rendered.is_empty() {
            rendered.push('\n');
        }
        rendered.push_str(&item);
    }
    Ok(rendered)
}

fn aliases_json(aliases: &[BoundedText]) -> Value {
    Value::Array(aliases.iter().map(|alias| Value::String(alias.as_str().to_owned())).collect())
}

fn optional_text_json(value: Option<&crate::core::BoundedText>) -> Value {
    value.map_or(Value::Null, |text| Value::String(text.as_str().to_owned()))
}

fn dialogue_examples_json(profile: &CharacterProfile) -> Value {
    Value::Array(
        profile
            .dialogue_examples
            .iter()
            .map(|example| {
                let mut value = Map::new();
                value.insert("situation".to_owned(), Value::String(example.situation.as_str().to_owned()));
                value.insert("response".to_owned(), Value::String(example.response.as_str().to_owned()));
                Value::Object(value)
            })
            .collect(),
    )
}

fn json_string(value: &str) -> String {
    Value::String(value.to_owned()).to_string()
}

fn truncated_json_string(value: &str, maximum_bytes: usize) -> Option<String> {
    let marker = json_string(TRUNCATION_MARKER);
    if marker.len() > maximum_bytes {
        return None;
    }
    let mut boundaries = value.char_indices().map(|(index, _)| index).collect::<Vec<_>>();
    boundaries.push(value.len());
    let mut low = 0;
    let mut high = boundaries.len();
    while low < high {
        let middle = low + (high - low) / 2;
        let candidate = json_string(&format!("{}{}", &value[..boundaries[middle]], TRUNCATION_MARKER));
        if candidate.len() <= maximum_bytes {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    let boundary = boundaries[low.saturating_sub(1)];
    Some(json_string(&format!("{}{}", &value[..boundary], TRUNCATION_MARKER)))
}

fn inconsistent(code: &'static str, message: impl Into<String>) -> PipelineError {
    PipelineError::inconsistent_data(PipelineStage::Generate, code, message)
}

#[cfg(test)]
#[path = "tests/generate_roles_tests.rs"]
mod tests;
