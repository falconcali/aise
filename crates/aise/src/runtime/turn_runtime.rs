use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_event::{TurnEvent, TurnEventSink};
use crate::core::turn_pipeline::TurnExecutionPipeline;
use crate::core::turn_trace::{PipelineData, SpanPayload};
use crate::core::turn_validation::ValidationDecision;
use crate::error::AiseError;
use crate::runtime::turn_pipeline_set::TurnPipelineSet;
use std::time::Instant;

pub struct TurnRuntime {
    pipeline_set: TurnPipelineSet,
}

impl TurnRuntime {
    pub fn new(pipeline_set: TurnPipelineSet) -> Self {
        Self { pipeline_set }
    }

    pub async fn run(&self, ctx: &mut TurnExecutionContext, sink: &dyn TurnEventSink) -> Result<(), AiseError> {
        self.execute(self.pipeline_set.initializer(), ctx, sink).await?;
        self.execute(self.pipeline_set.baseline_builder(), ctx, sink).await?;
        self.execute(self.pipeline_set.writer_planner(), ctx, sink).await?;

        if ctx.requires_retrieval()? {
            self.execute(self.pipeline_set.retrieval(), ctx, sink).await?;
        } else {
            ctx.skip_retrieval()?;
        }

        if ctx.requires_character_thinking()? {
            self.execute(self.pipeline_set.character_think(), ctx, sink).await?;
        } else {
            ctx.skip_character_thinking()?;
        }

        ctx.complete_context_preparation()?;
        self.execute(self.pipeline_set.story_generator(), ctx, sink).await?;

        loop {
            self.execute(self.pipeline_set.validation(), ctx, sink).await?;
            match ctx.validation_decision()? {
                ValidationDecision::Pass => break,
                ValidationDecision::Repair => {
                    ctx.consume_repair_round()?;
                    self.execute(self.pipeline_set.story_repairer(), ctx, sink).await?;
                }
                ValidationDecision::Reject => return Err(ctx.validation_error()?),
            }
        }

        self.execute(self.pipeline_set.committer(), ctx, sink).await?;
        ctx.committed_result()
            .map(|_| ())
            .ok_or_else(|| AiseError::InvariantViolation("committed turn missing committed result".into()))
    }

    async fn execute(
        &self,
        pipeline: &dyn TurnExecutionPipeline,
        ctx: &mut TurnExecutionContext,
        sink: &dyn TurnEventSink,
    ) -> Result<(), AiseError> {
        let stage = pipeline.stage();
        if ctx.control().cancellation().is_cancelled() {
            return Err(AiseError::Cancelled);
        }
        if Instant::now() >= ctx.control().deadline() {
            return Err(AiseError::TurnDeadlineExceeded);
        }
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
        outcome
    }
}
