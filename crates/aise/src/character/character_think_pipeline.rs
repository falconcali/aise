use crate::character::character_think_prompt::{
    CharacterThinkProjectionError, CharacterThinkPromptContextProjector, DefaultCharacterThinkPromptContextProjector,
};
use crate::config::{CharacterThinkConfig, ContextPreparationConfig};
use crate::domain::asset::validation::BoundedText;
use crate::domain::text::estimate_text_tokens;
use crate::domain::turn::CharacterDecision;
use crate::llm::gateway::LlmGateway;
use crate::llm::output_contract::{LlmOutputContract, LlmOutputViolation};
use crate::prompt::{PromptCompositionInput, PromptProfile};
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};
use crate::turn::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use async_trait::async_trait;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Instant;

const CHARACTER_DECISION_CONTRACT_NAME: &str = "character_decision.v1";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct CharacterDecisionDto {
    decision: String,
    suggested_utterance: String,
}

fn character_decision_contract(config: &CharacterThinkConfig) -> LlmOutputContract<CharacterDecisionDto> {
    let max_field_bytes = config.max_field_bytes;
    let schema = serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["decision", "suggested_utterance"],
        "properties": {
            "decision": {"type": "string", "minLength": 1, "maxLength": max_field_bytes},
            "suggested_utterance": {"type": "string", "maxLength": max_field_bytes}
        }
    });
    let compact_prompt_shape = format!(
        "Return exactly one JSON object: {{\"decision\": string (required, non-empty, <= {max_field_bytes} bytes), \"suggested_utterance\": string (use \"\" when absent, <= {max_field_bytes} bytes)}}. No other fields, no prose outside the object."
    );
    LlmOutputContract {
        name: CHARACTER_DECISION_CONTRACT_NAME,
        schema: Arc::new(schema),
        compact_prompt_shape: Arc::from(compact_prompt_shape.as_str()),
        validate: Arc::new(|dto: &CharacterDecisionDto| {
            if dto.decision.trim().is_empty() {
                Err(LlmOutputViolation::new(
                    CHARACTER_DECISION_CONTRACT_NAME,
                    "decision must be trim-non-empty",
                ))
            } else {
                Ok(())
            }
        }),
    }
}

pub struct CharacterThinkPipeline {
    gateway: Arc<LlmGateway>,
    projector: DefaultCharacterThinkPromptContextProjector,
    config: CharacterThinkConfig,
}

impl CharacterThinkPipeline {
    pub fn new(
        gateway: Arc<LlmGateway>,
        config: CharacterThinkConfig,
        context_config: ContextPreparationConfig,
    ) -> Self {
        Self {
            gateway,
            projector: DefaultCharacterThinkPromptContextProjector::new(config.clone(), context_config),
            config,
        }
    }
}

#[async_trait]
impl TurnExecutionPipeline for CharacterThinkPipeline {
    fn stage(&self) -> TurnStage {
        TurnStage::CharacterThink
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), TurnExecutionError> {
        let plan = ctx
            .plan()
            .ok_or_else(|| invariant("writer plan not set before character think"))?
            .clone();
        let mut decisions = Vec::with_capacity(plan.character_think_requests.len());
        for request in &plan.character_think_requests {
            let projection_started = Instant::now();
            let projection = self.projector.project(ctx, request).map_err(map_projection_error)?;
            let dialogue_example_count = projection.context.target_role.dialogue_examples.len();
            let dialogue_example_tokens = projection
                .context
                .target_role
                .dialogue_examples
                .iter()
                .map(|example| {
                    estimate_text_tokens(example.situation.as_str())
                        .saturating_add(estimate_text_tokens(example.response.as_str()))
                })
                .sum::<u64>();
            let omitted_dialogue_example_count = ctx
                .snapshot()
                .and_then(|snapshot| snapshot.role(&request.role_id))
                .map(|role| {
                    role.effective_profile
                        .dialogue_examples
                        .len()
                        .saturating_sub(dialogue_example_count)
                })
                .unwrap_or(0);
            let prompt_section_bytes = projection
                .rc_vars
                .as_map()
                .values()
                .filter_map(serde_json::Value::as_str)
                .map(str::len)
                .sum::<usize>();
            tracing::info!(
                story_id = %ctx.story_id(),
                turn_number = %ctx.turn_number(),
                target_role_id = %request.role_id,
                recent_story_segments = projection.context.story_continuity.recent_story.len(),
                known_rumor_count = projection.context.target_role.knowledge.known_rumors.len(),
                memory_count = projection.context.target_role.knowledge.memories.len(),
                character_impulse_count = projection.context.narrative_character_impulses.len(),
                dialogue_example_count,
                dialogue_example_tokens,
                omitted_dialogue_example_count,
                prompt_section_bytes,
                thinking_focus_bytes = projection.context.thinking_focus.as_str().len(),
                projection_duration_ms = projection_started.elapsed().as_millis(),
                "character think prompt projected"
            );
            let model_request = PromptCompositionInput {
                profile: PromptProfile::CharacterThink,
                rc_vars: projection.rc_vars,
                fti_vars: projection.fti_vars,
            };
            let max_output_tokens = ctx
                .budget()
                .remaining_output_tokens()
                .min(u64::from(self.config.max_output_tokens)) as u32;
            let scope = ctx.llm_call_scope(TurnStage::CharacterThink);
            let structured = self
                .gateway
                .complete_structured_composed(
                    scope,
                    model_request,
                    max_output_tokens,
                    crate::turn::turn_contract::LlmCallPurpose::CharacterThink,
                    character_decision_contract(&self.config),
                )
                .await
                .map_err(|error| {
                    TurnExecutionError::new(
                        TurnFailureKind::Llm,
                        "llm_error",
                        Some(TurnStage::CharacterThink),
                        error.to_string(),
                    )
                })?;
            let dto = structured.value;
            let decision_text = normalize_required_output(dto.decision, "decision", self.config.max_field_bytes)?;
            let suggested_utterance =
                normalize_optional_output(dto.suggested_utterance, "suggested_utterance", self.config.max_field_bytes)?;
            let total_bytes = enforce_total_output_budget(
                &decision_text,
                suggested_utterance.as_ref(),
                self.config.max_total_output_bytes,
            )?;
            tracing::info!(
                story_id = %ctx.story_id(),
                turn_number = %ctx.turn_number(),
                target_role_id = %request.role_id,
                decision_bytes = decision_text.as_str().len(),
                suggested_utterance_present = suggested_utterance.is_some(),
                suggested_utterance_bytes = suggested_utterance.as_ref().map(|value| value.as_str().len()).unwrap_or(0),
                output_bytes = total_bytes,
                "character decision normalized"
            );
            let decision = CharacterDecision {
                role_id: request.role_id.clone(),
                decision: decision_text,
                suggested_utterance,
            };
            decisions.push(decision);
        }
        ctx.set_character_decisions(decisions)
    }
}

fn invariant(message: impl Into<String>) -> TurnExecutionError {
    TurnExecutionError::invariant(message)
}

fn map_projection_error(error: CharacterThinkProjectionError) -> TurnExecutionError {
    let code = match error {
        CharacterThinkProjectionError::MissingStageState => "missing_stage_state",
        CharacterThinkProjectionError::UnknownRole { .. } => "unknown_role",
        CharacterThinkProjectionError::PlayerControlledRole { .. } => "player_controlled_role",
        CharacterThinkProjectionError::RequiredPromptDataExceedsBudget => "required_prompt_data_exceeds_budget",
        CharacterThinkProjectionError::InvalidPromptField => "invalid_prompt_field",
    };
    TurnExecutionError::new(
        TurnFailureKind::InvariantViolation,
        code,
        Some(TurnStage::CharacterThink),
        error.to_string(),
    )
}

fn model_output_invalid(message: impl Into<String>) -> TurnExecutionError {
    TurnExecutionError::new(
        TurnFailureKind::Llm,
        "model_output_invalid",
        Some(TurnStage::CharacterThink),
        message,
    )
}

fn normalize_required_output(
    value: String,
    field: &'static str,
    maximum_bytes: usize,
) -> Result<BoundedText, TurnExecutionError> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(model_output_invalid("character decision contains an empty required field"));
    }
    BoundedText::try_new(normalized.to_owned(), field, maximum_bytes)
        .map_err(|_| model_output_invalid("character decision field exceeds byte budget"))
}

fn normalize_optional_output(
    value: String,
    field: &'static str,
    maximum_bytes: usize,
) -> Result<Option<BoundedText>, TurnExecutionError> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Ok(None);
    }
    BoundedText::try_new(normalized.to_owned(), field, maximum_bytes)
        .map(Some)
        .map_err(|_| model_output_invalid("character decision field exceeds byte budget"))
}

fn enforce_total_output_budget(
    decision: &BoundedText,
    suggested_utterance: Option<&BoundedText>,
    maximum_bytes: usize,
) -> Result<usize, TurnExecutionError> {
    let total_bytes = decision
        .as_str()
        .len()
        .saturating_add(suggested_utterance.map(|value| value.as_str().len()).unwrap_or(0));
    if total_bytes > maximum_bytes {
        return Err(model_output_invalid("character decision exceeds total output byte budget"));
    }
    Ok(total_bytes)
}

#[cfg(test)]
#[path = "tests/character_think_pipeline_tests.rs"]
mod tests;
