use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_data::CharacterThought;
use crate::core::turn_data::character::CharacterThoughtOutput;
use crate::core::turn_error::{TurnExecutionError, TurnFailureKind};
use crate::core::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::domain::asset::validation::BoundedText;
use crate::llm::gateway::LlmGateway;
use crate::prompt::{CharacterThinkContext, ModelRequest};
use async_trait::async_trait;
use std::sync::Arc;

pub struct CharacterThinkPipeline {
    gateway: Arc<LlmGateway>,
    max_thought_bytes: usize,
}

impl CharacterThinkPipeline {
    pub fn new(gateway: Arc<LlmGateway>, max_thought_bytes: usize) -> Self {
        Self {
            gateway,
            max_thought_bytes,
        }
    }
}

#[async_trait]
impl TurnExecutionPipeline for CharacterThinkPipeline {
    fn stage(&self) -> TurnStage {
        TurnStage::CharacterThink
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), TurnExecutionError> {
        let baseline = ctx
            .baseline()
            .ok_or_else(|| invariant("baseline context not set before character think"))?
            .clone();
        let plan = ctx
            .plan()
            .ok_or_else(|| invariant("writer plan not set before character think"))?
            .clone();
        let snapshot = ctx
            .snapshot()
            .ok_or_else(|| invariant("snapshot not set before character think"))?
            .clone();
        let player_input = BoundedText::try_new(ctx.player_input().to_owned(), "player_input", 4096)
            .map_err(|_| invariant("player input exceeds bound"))?;
        let mut thoughts = Vec::with_capacity(plan.character_think_requests.len());
        for request in &plan.character_think_requests {
            let character = baseline
                .scene_characters
                .iter()
                .find(|candidate| candidate.character_id == request.character_id)
                .cloned()
                .or_else(|| {
                    if baseline.player_character.character_id == request.character_id {
                        Some(baseline.player_character.clone())
                    } else {
                        None
                    }
                });
            let Some(character) = character else {
                tracing::warn!(
                    turn_id = ctx.turn_id().as_str(),
                    story_id = ctx.story_id().as_str(),
                    character_id = request.character_id.as_str(),
                    "aise.character_think.skip_unknown_character"
                );
                continue;
            };
            let retrieved_context = ctx.retrieved().for_character(&request.character_id).to_vec();
            let current_perception = snapshot
                .current_perceptions()
                .iter()
                .filter(|perception| perception.character_id == request.character_id)
                .cloned()
                .collect();
            let impulses = plan
                .narrative_plan
                .character_impulses
                .iter()
                .filter(|impulse| impulse.target_character_id == request.character_id)
                .cloned()
                .collect();
            let model_request = ModelRequest::character_think(
                CharacterThinkContext {
                    character,
                    current_scene: baseline.current_scene.clone(),
                    retrieved_context,
                    current_perception,
                    impulses,
                    player_input: player_input.clone(),
                },
                ctx.budget().remaining_output_tokens().min(u64::from(u32::MAX)) as u32,
            );
            let scope = ctx.llm_call_scope(TurnStage::CharacterThink);
            let completion = self.gateway.complete_typed(scope, model_request).await.map_err(|error| {
                TurnExecutionError::new(
                    TurnFailureKind::Llm,
                    "llm_error",
                    Some(TurnStage::CharacterThink),
                    error.to_string(),
                )
            })?;
            let output: CharacterThoughtOutput = serde_json::from_str(&completion.text)
                .map_err(|_| invariant("character thought output is not valid JSON"))?;
            let thought = CharacterThought {
                character_id: request.character_id.clone(),
                perception: output.perception,
                emotion: output.emotion,
                goal: output.goal,
                possible_action: output.possible_action,
            };
            let serialized = serde_json::to_string(&thought).map_err(|_| invariant("thought serialize failed"))?;
            if serialized.len() > self.max_thought_bytes {
                return Err(invariant("character thought exceeds byte budget"));
            }
            thoughts.push(thought);
        }
        ctx.set_character_thoughts(thoughts)
    }
}

fn invariant(message: impl Into<String>) -> TurnExecutionError {
    TurnExecutionError::invariant(message)
}

#[cfg(test)]
#[path = "tests/character_think_pipeline_tests.rs"]
mod tests;
