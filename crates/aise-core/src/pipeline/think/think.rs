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
    pub result: String,
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
        _story_ctx: &StoryContext,
        _control: &TurnControl,
        _observation: &Observation,
    ) -> Result<Self::Output, PipelineError> {
        Ok(ThinkOutput {
            result: input.query.into(),
        })
    }
}
