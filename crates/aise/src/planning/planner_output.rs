use crate::config::PlannerConfig;
use crate::llm::output_contract::{LlmOutputContract, LlmOutputViolation};
use serde::Deserialize;
use std::sync::Arc;

pub const WRITER_PLANNER_CONTRACT_NAME: &str = "writer_planner_output.v2";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriterPlannerOutputDto {
    pub interpreted_player_contribution: InterpretedPlayerContributionDto,
    pub story_goal: String,
    pub writer_context_gaps: Vec<PlannerWriterContextGapDto>,
    pub character_context_gaps: Vec<PlannerCharacterContextGapDto>,
    pub character_think_requests: Vec<CharacterThinkRequestDto>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterpretedPlayerContributionDto {
    pub units: Vec<PlayerContributionUnitDto>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerContributionUnitDto {
    pub kind: PlayerContributionKindDto,
    pub content: String,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerContributionKindDto {
    Speech,
    Action,
    PrivateState,
    RequestedOutcome,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerWriterContextGapDto {
    pub target_id: String,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerCharacterContextGapDto {
    pub role_id: String,
    pub target_id: String,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterThinkRequestDto {
    pub role_id: String,
    pub reason: String,
}

pub fn writer_planner_output_schema(config: &PlannerConfig) -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["interpreted_player_contribution", "story_goal", "writer_context_gaps", "character_context_gaps", "character_think_requests"],
        "properties": {
            "interpreted_player_contribution": {
                "type": "object",
                "additionalProperties": false,
                "required": ["units"],
                "properties": {
                    "units": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": config.max_player_contribution_units,
                        "items": {
                            "type": "object",
                            "additionalProperties": false,
                            "required": ["kind", "content"],
                            "properties": {
                                "kind": {
                                    "type": "string",
                                    "enum": ["speech", "action", "private_state", "requested_outcome"]
                                },
                                "content": {
                                    "type": "string",
                                    "minLength": 1,
                                    "maxLength": config.max_interpreted_player_contribution_bytes
                                }
                            }
                        }
                    }
                }
            },
            "story_goal": {"type": "string", "minLength": 1, "maxLength": config.max_goal_bytes},
            "writer_context_gaps": {
                "type": "array",
                "maxItems": config.max_context_gaps,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["target_id", "reason"],
                    "properties": {
                        "target_id": {"type": "string", "minLength": 1, "maxLength": 256},
                        "reason": {"type": "string", "minLength": 1, "maxLength": config.max_reason_bytes}
                    }
                }
            },
            "character_context_gaps": {
                "type": "array",
                "maxItems": config.max_context_gaps,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["role_id", "target_id", "reason"],
                    "properties": {
                        "role_id": {"type": "string", "minLength": 1},
                        "target_id": {"type": "string", "minLength": 1, "maxLength": 256},
                        "reason": {"type": "string", "minLength": 1, "maxLength": config.max_reason_bytes}
                    }
                }
            },
            "character_think_requests": {
                "type": "array",
                "maxItems": config.max_character_think_requests,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["role_id", "reason"],
                    "properties": {
                        "role_id": {"type": "string", "minLength": 1},
                        "reason": {"type": "string", "minLength": 1, "maxLength": config.max_reason_bytes}
                    }
                }
            }
        }
    })
}

pub fn writer_planner_compact_prompt_shape(config: &PlannerConfig) -> String {
    format!(
        "Return exactly one JSON object: {{\"interpreted_player_contribution\": {{\"units\": array (required, 1..{units} ordered items; each {{\"kind\": one of speech/action/private_state/requested_outcome, \"content\": non-empty string}}; aggregate content <= {bytes} bytes)}}, \"story_goal\": string (required, non-empty, <= {goal} bytes), \"writer_context_gaps\": array (<= {gaps} items, each {{\"target_id\": string, \"reason\": string}}), \"character_context_gaps\": array (<= {gaps} items, each {{\"role_id\": string, \"target_id\": string, \"reason\": string}}), \"character_think_requests\": array (<= {think} items, each {{\"role_id\": string, \"reason\": string}})}}. No other fields, no prose outside the object.",
        units = config.max_player_contribution_units,
        bytes = config.max_interpreted_player_contribution_bytes,
        goal = config.max_goal_bytes,
        gaps = config.max_context_gaps,
        think = config.max_character_think_requests,
    )
}

pub fn writer_planner_contract(config: &PlannerConfig) -> LlmOutputContract<WriterPlannerOutputDto> {
    let schema = writer_planner_output_schema(config);
    let compact_prompt_shape = writer_planner_compact_prompt_shape(config);
    let max_goal_bytes = config.max_goal_bytes;
    let max_context_gaps = config.max_context_gaps;
    let max_character_think_requests = config.max_character_think_requests;
    let max_reason_bytes = config.max_reason_bytes;
    let max_player_contribution_units = config.max_player_contribution_units;
    let max_interpreted_player_contribution_bytes = config.max_interpreted_player_contribution_bytes;
    LlmOutputContract {
        name: WRITER_PLANNER_CONTRACT_NAME,
        schema: Arc::new(schema),
        compact_prompt_shape: Arc::from(compact_prompt_shape.as_str()),
        validate: Arc::new(move |dto: &WriterPlannerOutputDto| {
            validate_writer_planner_output(
                dto,
                max_goal_bytes,
                max_context_gaps,
                max_character_think_requests,
                max_reason_bytes,
                max_player_contribution_units,
                max_interpreted_player_contribution_bytes,
            )
        }),
    }
}

fn validate_writer_planner_output(
    dto: &WriterPlannerOutputDto,
    max_goal_bytes: usize,
    max_context_gaps: usize,
    max_character_think_requests: usize,
    max_reason_bytes: usize,
    max_player_contribution_units: usize,
    max_interpreted_player_contribution_bytes: usize,
) -> Result<(), LlmOutputViolation> {
    let violation = |reason: &str| Err(LlmOutputViolation::new(WRITER_PLANNER_CONTRACT_NAME, reason.to_owned()));
    if dto.interpreted_player_contribution.units.is_empty()
        || dto.interpreted_player_contribution.units.len() > max_player_contribution_units
    {
        return violation("interpreted_player_contribution unit count exceeds the configured bounds");
    }
    if dto
        .interpreted_player_contribution
        .units
        .iter()
        .any(|unit| unit.content.trim().is_empty())
    {
        return violation("interpreted_player_contribution unit content must be non-empty");
    }
    if dto
        .interpreted_player_contribution
        .units
        .iter()
        .map(|unit| unit.content.len())
        .fold(0usize, usize::saturating_add)
        > max_interpreted_player_contribution_bytes
    {
        return violation("interpreted_player_contribution content exceeds the configured byte budget");
    }
    if dto.story_goal.trim().is_empty() || dto.story_goal.len() > max_goal_bytes {
        return violation("story_goal must be trim-non-empty and within the configured byte budget");
    }
    if dto.writer_context_gaps.len() > max_context_gaps || dto.character_context_gaps.len() > max_context_gaps {
        return violation("context gap count exceeds the configured maximum");
    }
    if dto.character_think_requests.len() > max_character_think_requests {
        return violation("character think request count exceeds the configured maximum");
    }
    for gap in &dto.writer_context_gaps {
        if gap.target_id.trim().is_empty() || gap.reason.trim().is_empty() || gap.reason.len() > max_reason_bytes {
            return violation("writer context gap has an empty or oversized field");
        }
    }
    for gap in &dto.character_context_gaps {
        if gap.role_id.trim().is_empty()
            || gap.target_id.trim().is_empty()
            || gap.reason.trim().is_empty()
            || gap.reason.len() > max_reason_bytes
        {
            return violation("character context gap has an empty or oversized field");
        }
    }
    for request in &dto.character_think_requests {
        if request.role_id.trim().is_empty()
            || request.reason.trim().is_empty()
            || request.reason.len() > max_reason_bytes
        {
            return violation("character think request has an empty or oversized field");
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/planner_output_tests.rs"]
mod tests;
