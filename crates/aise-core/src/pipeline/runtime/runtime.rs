use crate::core::{CommittedTurnInfo, EngineError, TurnControl, TurnEventSink, TurnRequest, TurnResult, StoryContext};
use crate::trace::Trace;
use crate::pipeline::common::{PipelineRunner};
use crate::pipeline::baseline::{ BaselinePipeline, BaselineInput, BaselineOutput };
use crate::pipeline::plan::{ PlanPipeline, PlanInput, PlanOutput };

pub struct Runtime {
    baseline: BaselinePipeline,
    plan: PlanPipeline
}

impl Runtime {
    pub fn new() -> Self {
        Self {
            baseline: BaselinePipeline,
            plan: PlanPipeline,
        }
    }

    pub async fn run_turn(
        &self,
        turn_request: TurnRequest,
        turn_control: TurnControl,
        sink: &dyn TurnEventSink,
        trace: &Trace,
    ) -> Result<TurnResult, EngineError> {
        let pipeline_runner = PipelineRunner {
            control: &turn_control,
            sink,
            trace
        };

        let baseline_input: BaselineInput = BaselineInput {
            story_ctx: StoryContext { story_id: turn_request.story_id.clone() },
            player_input: turn_request.player_input.clone(),
        };

        let baseline_output = pipeline_runner.run(&self.baseline, baseline_input).await
            .map_err(|error| EngineError::Turn {
                message: error.to_string(),
            })?;

        let plan_input = PlanInput {
            story_ctx: StoryContext { story_id: turn_request.story_id.clone() },
            player_contribution: baseline_output.player_contribution,
        };

        let plan_output = pipeline_runner.run(&self.plan, plan_input).await
            .map_err(|error| EngineError::Turn {
                message: error.to_string(),
            })?;

        Ok(TurnResult {
            result: CommittedTurnInfo {
                turn_number: 1,
                story_revision: 1,
                story_text: plan_output.plan,
            },
            replayed: false,
        })
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}
