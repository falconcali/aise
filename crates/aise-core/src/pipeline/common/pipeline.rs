use crate::core::{StoryContext, TurnControl, TurnEventSink};
use crate::pipeline::common::PipelineError;
use crate::pipeline::common::pipeline_trace;
use crate::trace::{Observation, Trace};
use serde::Serialize;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineStage {
    Baseline,
    Plan,
    Retrieval,
    Think,
    Generate,
    Validate,
    Repair,
    Extract,
    Commit,
}

impl PipelineStage {
    pub fn as_str(&self) -> &'static str {
        match self {
            PipelineStage::Baseline => "baseline",
            PipelineStage::Plan => "plan",
            PipelineStage::Retrieval => "retrieval",
            PipelineStage::Think => "think",
            PipelineStage::Generate => "generate",
            PipelineStage::Validate => "validate",
            PipelineStage::Repair => "repair",
            PipelineStage::Extract => "extract",
            PipelineStage::Commit => "commit",
        }
    }
}

impl fmt::Display for PipelineStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

pub struct PipelineRunner<'a> {
    pub control: &'a TurnControl,
    pub sink: &'a dyn TurnEventSink,
    pub trace: &'a Trace,
}

impl PipelineRunner<'_> {
    pub async fn run<P: Pipeline>(
        &self,
        pipeline: &P,
        story_ctx: &StoryContext,
        input: P::Input,
    ) -> Result<P::Output, PipelineError> {
        let observation = pipeline_trace::begin_observation(self.trace, pipeline, &input);
        let result = pipeline.execute(story_ctx, input, self.control, self.sink, &observation).await;
        pipeline_trace::finish_observation(observation, pipeline, &result);
        result
    }
}

pub trait Pipeline: Send + Sync {
    type Input: Send + Serialize;
    type Output: Send + Serialize;

    fn stage(&self) -> PipelineStage;

    async fn execute(
        &self,
        story_ctx: &StoryContext,
        input: Self::Input,
        control: &TurnControl,
        sink: &dyn TurnEventSink,
        observation: &Observation,
    ) -> Result<Self::Output, PipelineError>;
}
