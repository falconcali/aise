use crate::core::{CommittedTurnInfo, TurnControl, TurnEventSink, TurnRequest, TurnResult, StoryContext};
use crate::pipeline::common::{PipelineRunner, PipelineError};
use crate::pipeline::baseline::{ BaselinePipeline, BaselineInput };
use crate::pipeline::plan::{ PlanPipeline, PlanInput };
use crate::trace::Trace;

pub struct Runtime {
    baseline: BaselinePipeline,
    plan: PlanPipeline
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
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
    ) -> Result<TurnResult, PipelineError> {
        let pipeline_runner = PipelineRunner {
            control: &turn_control,
            sink,
            trace
        };

        let baseline_input = BaselineInput {
            story_ctx: StoryContext { story_id: turn_request.story_id.clone() },
            player_input: turn_request.player_input.clone(),
        };

        let baseline_output = pipeline_runner.run(&self.baseline, baseline_input).await?;

        let plan_input = PlanInput {
            story_ctx: StoryContext { story_id: turn_request.story_id.clone() },
            player_contribution: baseline_output.player_contribution,
        };

        let plan_output = pipeline_runner.run(&self.plan, plan_input).await?;

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
