use crate::core::{Change, StoryContext, StorySummary, TurnControl, TurnEventSink, TurnNumber};
use crate::llm::LlmGateway;
use crate::persistence::StoryStore;
use crate::pipeline::common::{Pipeline, PipelineError, PipelineStage};
use crate::pipeline::summary::{SummaryConfig, summary_llm, summary_prompt, summary_trace};
use crate::prompt::Prompt;
use crate::trace::Observation;
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
pub struct SummaryInput {
    pub pending_turn_number: TurnNumber,
}

#[derive(Serialize)]
pub struct SummaryOutput {
    pub summary: Change<StorySummary>,
}

pub struct SummaryPipeline {
    gateway: Arc<LlmGateway>,
    prompt: Arc<Prompt>,
    story_store: Arc<dyn StoryStore>,
    config: SummaryConfig,
}

impl SummaryPipeline {
    pub fn new(
        gateway: Arc<LlmGateway>,
        prompt: Arc<Prompt>,
        story_store: Arc<dyn StoryStore>,
        config: SummaryConfig,
    ) -> Self {
        Self {
            gateway,
            prompt,
            story_store,
            config,
        }
    }

    async fn summarize_story(
        &self,
        story_ctx: &StoryContext,
        covered_turns: usize,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<String, PipelineError> {
        let rendered_prompt = summary_prompt::summarize_story(
            self.prompt.as_ref(),
            story_ctx,
            covered_turns,
            self.story_store.as_ref(),
            observation,
        )
        .await?;

        let summary_text =
            summary_llm::summarize_story(self.gateway.as_ref(), rendered_prompt.into_messages(), control, observation)
                .await
                .map_err(|error| PipelineError::new(PipelineStage::Summary, error.to_string()))?;

        let summary_text = summary_text.trim();
        if summary_text.is_empty() {
            return Err(PipelineError::new(PipelineStage::Summary, "summary model returned empty text"));
        }
        Ok(summary_text.to_owned())
    }

    fn covered_turn_count(&self, story_ctx: &StoryContext) -> usize {
        if self.config.summary_turn_count == 0 {
            return 0;
        }
        (story_ctx.rencent_turns.len() + 1).saturating_sub(self.config.summary_turn_count)
    }
}

impl Pipeline for SummaryPipeline {
    type Input = SummaryInput;
    type Output = SummaryOutput;

    fn stage(&self) -> PipelineStage {
        PipelineStage::Summary
    }

    async fn execute(
        &self,
        input: Self::Input,
        story_ctx: &StoryContext,
        control: &TurnControl,
        _sink: &dyn TurnEventSink,
        observation: &Observation,
    ) -> Result<Self::Output, PipelineError> {
        let covered_turns = self.covered_turn_count(story_ctx);
        if covered_turns == 0 {
            return Ok(SummaryOutput {
                summary: Change::Unchanged,
            });
        }

        let covered_through = story_ctx.rencent_turns[covered_turns - 1].turn_number.clone();
        let summary_observation =
            summary_trace::begin_summary(observation, &input.pending_turn_number, covered_turns, &covered_through);
        let result = self
            .summarize_story(story_ctx, covered_turns, control, &summary_observation)
            .await;
        summary_trace::finish_summary(summary_observation, &result);
        result.map(|text| SummaryOutput {
            summary: Change::Replaced(StorySummary { text, covered_through }),
        })
    }
}

#[cfg(test)]
#[path = "tests/summary_tests.rs"]
mod tests;
