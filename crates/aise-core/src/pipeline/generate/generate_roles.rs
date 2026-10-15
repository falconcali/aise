use crate::core::{CharacterDecision, ResolvedRole, RoleId};
use crate::pipeline::common::{PipelineError, PipelineStage};
use crate::pipeline::generate::GenerateConfig;
use serde_json::{Map, Value};
use std::collections::HashSet;

const TRUNCATION_MARKER: &str = "[truncated]";

#[derive(Debug)]
pub(super) struct GenerateRoleContext {
    pub roles: String,
    pub decisions: String,
}

pub(super) fn project_role_context(
    roles: &[ResolvedRole],
    decisions: &[CharacterDecision],
    config: &GenerateConfig,
) -> Result<GenerateRoleContext, PipelineError> {
    validate_decisions(roles, decisions)?;
    Ok(GenerateRoleContext {
        roles: render_roles(roles, config)?,
        decisions: render_decisions(decisions, config)?,
    })
}

pub(super) fn validate_player_decisions(
    decisions: &[CharacterDecision],
    player_role: &RoleId,
) -> Result<(), PipelineError> {
    if decisions.iter().any(|decision| &decision.role_id == player_role) {
        return Err(inconsistent(
            "generate_player_character_decision_forbidden",
            "Character Decision targets the player role",
        ));
    }
    Ok(())
}

fn validate_decisions(roles: &[ResolvedRole], decisions: &[CharacterDecision]) -> Result<(), PipelineError> {
    let mut seen = HashSet::with_capacity(decisions.len());
    for decision in decisions {
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

fn render_roles(roles: &[ResolvedRole], config: &GenerateConfig) -> Result<String, PipelineError> {
    let mut rendered = String::new();
    for role in roles {
        let role = render_role(role, config)?;
        let separator_bytes = if rendered.is_empty() { 0 } else { 2 };
        if rendered.len().saturating_add(separator_bytes).saturating_add(role.len()) > config.max_role_context_bytes {
            return Err(inconsistent(
                "generate_role_context_too_large",
                "resolved role context exceeds the aggregate byte budget",
            ));
        }
        if !rendered.is_empty() {
            rendered.push_str("\n\n");
        }
        rendered.push_str(&role);
    }
    Ok(rendered)
}

fn render_role(role: &ResolvedRole, config: &GenerateConfig) -> Result<String, PipelineError> {
    let mut rendered = [
        format!("role_id: {}", json_string(role.role_id.as_str())),
        format!("name: {}", json_string(role.name.as_str())),
        format!("aliases: {}", aliases_json(role)),
        format!("profile.appearance: {}", optional_text_json(role.profile.appearance.as_ref())),
        format!("profile.personality: {}", optional_text_json(role.profile.personality.as_ref())),
        format!(
            "profile.speaking_style: {}",
            optional_text_json(role.profile.speaking_style.as_ref())
        ),
        format!("profile.dialogue_examples: {}", dialogue_examples_json(role)),
    ]
    .join("\n");
    let backstory_prefix = "\nbackstory: ";
    let memories_header = "\nseed_memories:";
    if rendered.len() > config.max_role_bytes_per_role {
        return Err(inconsistent(
            "generate_role_projection_too_large",
            "mandatory role identity and profile exceed the per-role byte budget",
        ));
    }
    let full_backstory = json_string(role.backstory.as_str());
    let full_bytes = rendered
        .len()
        .saturating_add(backstory_prefix.len())
        .saturating_add(full_backstory.len())
        .saturating_add(memories_header.len());
    let backstory = if full_bytes <= config.max_role_bytes_per_role {
        full_backstory
    } else {
        let minimum_backstory = json_string(TRUNCATION_MARKER);
        let minimum_bytes = rendered
            .len()
            .saturating_add(backstory_prefix.len())
            .saturating_add(minimum_backstory.len())
            .saturating_add(memories_header.len());
        if minimum_bytes > config.max_role_bytes_per_role {
            return Err(inconsistent(
                "generate_role_projection_too_large",
                "truncated role backstory cannot fit the per-role byte budget",
            ));
        }
        let available = config
            .max_role_bytes_per_role
            .saturating_sub(rendered.len())
            .saturating_sub(backstory_prefix.len())
            .saturating_sub(memories_header.len());
        truncated_json_string(role.backstory.as_str(), available).ok_or_else(|| {
            inconsistent(
                "generate_role_projection_too_large",
                "truncated role backstory cannot fit the per-role byte budget",
            )
        })?
    };
    rendered.push_str(backstory_prefix);
    rendered.push_str(&backstory);
    rendered.push_str(memories_header);
    for (memory_id, memory) in role.seed_memories.iter().take(config.max_seed_memories_per_role) {
        let item = format!(
            "\n- memory_id: {}\n  content: {}\n  retrieval_hint: {}",
            json_string(memory_id.as_str()),
            json_string(memory.content.as_str()),
            json_string(memory.retrieval.as_str())
        );
        if rendered.len().saturating_add(item.len()) > config.max_role_bytes_per_role {
            break;
        }
        rendered.push_str(&item);
    }
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

fn aliases_json(role: &ResolvedRole) -> Value {
    Value::Array(
        role.aliases
            .iter()
            .map(|alias| Value::String(alias.as_str().to_owned()))
            .collect(),
    )
}

fn optional_text_json(value: Option<&crate::core::BoundedText>) -> Value {
    value.map_or(Value::Null, |text| Value::String(text.as_str().to_owned()))
}

fn dialogue_examples_json(role: &ResolvedRole) -> Value {
    Value::Array(
        role.profile
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
