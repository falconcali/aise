use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_event::{TurnEvent, TurnEventSink};
use crate::core::turn_pipeline::TurnExecutionPipeline;
use crate::core::turn_trace::{PipelineData, SpanPayload};
use crate::error::AiseError;

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
            let pending = ctx.trace().begin_span("aise.pipeline", stage.as_str());
            let outcome = pipeline.execute(ctx).await;
            let payload = match &outcome {
                Ok(()) => SpanPayload::Pipeline(PipelineData {
                    stage: stage.as_str().to_owned(),
                    status: "ok".into(),
                    error: None,
                }),
                Err(error) => SpanPayload::Pipeline(PipelineData {
                    stage: stage.as_str().to_owned(),
                    status: "error".into(),
                    error: Some(error.to_string()),
                }),
            };
            ctx.trace().end_span_with(pending, &payload);
            outcome?;
        }
        Ok(())
    }
}
