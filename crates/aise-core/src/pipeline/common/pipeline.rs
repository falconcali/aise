use crate::core::{TurnControl, TurnEventSink};
use crate::pipeline::common::PipelineError;
use crate::trace::{Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationStatus, Trace};
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
    pub async fn run<P>(&self, pipeline: &P, input: P::Input) -> Result<P::Output, PipelineError>
    where
        P: Pipeline + ?Sized,
    {
        let observation = self
            .trace
            .begin_observation_with_name(pipeline.stage().as_str(), ObservationKind::Span);
        let result = pipeline.execute(input, self.control, self.sink, &observation).await;
        let outcome = match &result {
            Ok(_) => ObservationOutcome {
                status: ObservationStatus::Ok,
                ..ObservationOutcome::default()
            },
            Err(error) => ObservationOutcome {
                status: ObservationStatus::Error,
                error: Some(ObservationError {
                    code: "pipeline_stage_failed".into(),
                    failure_kind: "pipeline".into(),
                    stage: Some(pipeline.stage().as_str().into()),
                    message: error.to_string(),
                }),
                ..ObservationOutcome::default()
            },
        };
        observation.finish(outcome);
        result
    }
}

pub trait Pipeline: Send + Sync {
    type Input: Send;
    type Output: Send;

    fn stage(&self) -> PipelineStage;

    async fn execute(
        &self,
        input: Self::Input,
        control: &TurnControl,
        sink: &dyn TurnEventSink,
        observation: &Observation,
    ) -> Result<Self::Output, PipelineError>;
}
