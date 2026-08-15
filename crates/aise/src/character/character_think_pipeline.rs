use crate::character::character_think_prompt::{
    CharacterThinkPromptContextProjector, DefaultCharacterThinkPromptContextProjector,
};
use crate::config::CharacterThinkConfig;
use crate::domain::asset::validation::BoundedText;
use crate::domain::turn::CharacterDecision;
use crate::domain::turn::character::CharacterDecisionOutput;
use crate::llm::gateway::LlmGateway;
use crate::prompt::{PromptCompositionInput, PromptProfile};
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};
use crate::turn::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Instant;

pub struct CharacterThinkPipeline {
    gateway: Arc<LlmGateway>,
    projector: DefaultCharacterThinkPromptContextProjector,
    config: CharacterThinkConfig,
}

impl CharacterThinkPipeline {
    pub fn new(gateway: Arc<LlmGateway>, config: CharacterThinkConfig) -> Self {
        Self {
            gateway,
            projector: DefaultCharacterThinkPromptContextProjector::new(config.clone()),
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
            let projection = self
                .projector
                .project(ctx, request)
                .map_err(|error| invariant(error.to_string()))?;
            tracing::info!(
                story_id = %ctx.story_id(),
                turn_id = %ctx.turn_id(),
                target_role_id = %request.role_id,
                recent_story_segments = projection.context.story_continuity.recent_story.len(),
                character_knowledge_count = projection.context.relevant_character_knowledge.len(),
                character_impulse_count = projection.context.narrative_character_impulses.len(),
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
            let completion = self
                .gateway
                .complete_composed(
                    scope,
                    model_request,
                    max_output_tokens,
                    crate::turn::turn_contract::LlmCallPurpose::CharacterThink,
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
            let output: CharacterDecisionOutput = serde_json::from_str(&completion.text)
                .map_err(|_| model_output_invalid("character decision output is not valid JSON"))?;
            let decision_text = normalize_required_output(output.decision, "decision", self.config.max_field_bytes)?;
            let suggested_utterance = normalize_optional_output(
                output.suggested_utterance,
                "suggested_utterance",
                self.config.max_field_bytes,
            )?;
            let total_bytes = enforce_total_output_budget(
                &decision_text,
                suggested_utterance.as_ref(),
                self.config.max_total_output_bytes,
            )?;
            tracing::info!(
                story_id = %ctx.story_id(),
                turn_id = %ctx.turn_id(),
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

fn model_output_invalid(message: impl Into<String>) -> TurnExecutionError {
    TurnExecutionError::new(
        TurnFailureKind::Llm,
        "model_output_invalid",
        Some(TurnStage::CharacterThink),
        message,
    )
}

fn normalize_required_output(
    value: BoundedText,
    field: &'static str,
    maximum_bytes: usize,
) -> Result<BoundedText, TurnExecutionError> {
    let normalized = value.as_str().trim();
    if normalized.is_empty() {
        return Err(model_output_invalid("character decision contains an empty required field"));
    }
    BoundedText::try_new(normalized.to_owned(), field, maximum_bytes)
        .map_err(|_| model_output_invalid("character decision field exceeds byte budget"))
}

fn normalize_optional_output(
    value: Option<BoundedText>,
    field: &'static str,
    maximum_bytes: usize,
) -> Result<Option<BoundedText>, TurnExecutionError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let normalized = value.as_str().trim();
    if normalized.is_empty() {
        return Err(model_output_invalid("character decision contains an empty optional field"));
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
