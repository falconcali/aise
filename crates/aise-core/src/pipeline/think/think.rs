use crate::core::{StoryContext, TurnControl};
use crate::pipeline::common::{Pipeline, PipelineError, PipelineStage};
use crate::trace::Observation;
use serde::Serialize;

#[derive(Serialize)]
pub struct ThinkInput<'a> {
    pub query: &'a str,
}

#[derive(Serialize)]
pub struct ThinkOutput {
    pub result: String, // 不仅输出决策，也可以按planner的要求输出记忆，用于更好的续写故事。
}

pub struct ThinkPipeline;

impl<'a> Pipeline<ThinkInput<'a>> for ThinkPipeline {
    type Output = ThinkOutput;

    fn stage(&self) -> PipelineStage {
        PipelineStage::Think
    }

    async fn execute(
        &self,
        input: ThinkInput<'a>,
        story_ctx: &StoryContext,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<Self::Output, PipelineError> {
        Ok(ThinkOutput {
            result: input.query.to_owned(),
        })
    }
}
