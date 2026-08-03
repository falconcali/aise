use crate::error::AiseError;
use crate::runtime::pipeline::TurnExecutionPipeline;
use crate::runtime::trace::TraceEvent;
use crate::runtime::turn_execution_ctx::TurnExecutionContext;
use std::time::Instant;

pub struct TurnRuntime {
    pipelines: Vec<Box<dyn TurnExecutionPipeline>>,
}

impl TurnRuntime {
    pub fn new(pipelines: Vec<Box<dyn TurnExecutionPipeline>>) -> Self {
        Self { pipelines }
    }

    pub async fn run(&self, ctx: &mut TurnExecutionContext) -> Result<(), AiseError> {
        for pipeline in &self.pipelines {
            let start = Instant::now();
            pipeline.execute(ctx).await?;
            ctx.trace.events.push(TraceEvent {
                stage: pipeline.stage(),
                elapsed: start.elapsed(),
            });
        }
        Ok(())
    }
}
