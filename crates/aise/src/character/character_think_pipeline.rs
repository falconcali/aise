use crate::character::character_model::CharacterThought;
use crate::error::AiseError;
use crate::runtime::pipeline::TurnExecutionPipeline;
use crate::runtime::turn_execution_ctx::TurnExecutionContext;
use async_trait::async_trait;

#[derive(Default)]
pub struct CharacterThinkPipeline;

#[async_trait]
impl TurnExecutionPipeline for CharacterThinkPipeline {
    fn stage(&self) -> &'static str {
        "character_think"
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), AiseError> {
        let plan = ctx.plan.clone().unwrap_or_default();
        if !plan.need_character_thinking {
            ctx.character_thoughts.clear();
            return Ok(());
        }
        let requested: Vec<_> = plan.character_requests;
        ctx.character_thoughts = requested
            .into_iter()
            .map(|character_id| CharacterThought {
                character_id,
                perception: ctx.player_input.clone(),
                emotion: String::new(),
                goal: String::new(),
                possible_action: String::new(),
            })
            .collect();
        Ok(())
    }
}
