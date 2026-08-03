use crate::error::AiseError;
use crate::runtime::event::{TurnEvent, TurnEventSink};
use crate::runtime::pipeline::TurnExecutionPipeline;
use crate::runtime::trace::{PipelineData, SpanPayload};
use crate::runtime::turn_execution_ctx::TurnExecutionContext;

pub struct TurnRuntime {
    pipelines: Vec<Box<dyn TurnExecutionPipeline>>,
}

impl TurnRuntime {
    pub fn new(pipelines: Vec<Box<dyn TurnExecutionPipeline>>) -> Self {
        Self { pipelines }
    }

    pub async fn run(&self, ctx: &mut TurnExecutionContext, sink: &dyn TurnEventSink) -> Result<(), AiseError> {
        for pipeline in &self.pipelines {
            let stage = pipeline.stage();
            sink.emit(TurnEvent::StageStarted(stage));
            let pending = ctx.trace.begin_span("aise.pipeline", stage);
            let outcome = pipeline.execute(ctx).await;
            let payload = match &outcome {
                Ok(()) => SpanPayload::Pipeline(PipelineData {
                    stage: stage.to_owned(),
                    status: "ok".into(),
                    error: None,
                }),
                Err(error) => SpanPayload::Pipeline(PipelineData {
                    stage: stage.to_owned(),
                    status: "error".into(),
                    error: Some(error.to_string()),
                }),
            };
            ctx.trace.end_span_with(pending, &payload);
            outcome?;
        }
        Ok(())
    }
}
