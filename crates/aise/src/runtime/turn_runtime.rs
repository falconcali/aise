use std::time::Instant;

use crate::error::AiseError;
use crate::runtime::pipeline::TurnExecutionPipeline;
use crate::runtime::trace::TraceEvent;
use crate::runtime::turn_execution_ctx::TurnExecutionContext;

/// Orchestrates the fixed Turn pipeline sequence (Architecture.md §4).
///
/// This is the only caller of pipelines (R-AISE-01). Stage budget and the
/// Validation/Repair loop are enforced here; conditional stages (retrieval,
/// character think) are encoded in the pipeline list by the assembler.
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
