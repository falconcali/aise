use crate::core::{CommittedTurnInfo, TurnControl, TurnEventSink, TurnRequest, TurnResult, StoryContext};
use crate::pipeline::common::{PipelineRunner, PipelineError};
use crate::pipeline::baseline::{ BaselinePipeline, BaselineInput };
use crate::pipeline::plan::{ PlanPipeline, PlanInput };
use crate::pipeline::retrieval::{ RetrievalPipeline, RetrievalInput };
use crate::pipeline::think::{ ThinkPipeline, ThinkInput };
use crate::pipeline::generate::{ GeneratePipeline, GenerateInput };
use crate::pipeline::validate::{ ValidatePipeline, ValidateInput };
use crate::pipeline::repair::{ RepairPipeline, RepairInput };
use crate::pipeline::extract::{ ExtractPipeline, ExtractInput };
use crate::pipeline::commit::{ CommitPipeline, CommitInput };
use crate::trace::Trace;

pub struct Runtime {
    baseline: BaselinePipeline,
    plan: PlanPipeline,
    retrieval: RetrievalPipeline,
    think: ThinkPipeline,
    generate: GeneratePipeline,
    validate: ValidatePipeline,
    repair: RepairPipeline,
    extract: ExtractPipeline,
    commit: CommitPipeline,
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
            retrieval: RetrievalPipeline,
            think: ThinkPipeline,
            generate: GeneratePipeline,
            validate: ValidatePipeline { score_configs: vec![] },
            repair: RepairPipeline,
            extract: ExtractPipeline,
            commit: CommitPipeline,
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

        let retrieval_input = RetrievalInput {
            query: plan_output.plan,
        };

        let retrieval_output = pipeline_runner.run(&self.retrieval, retrieval_input).await?;

        let think_input = ThinkInput {
            query: retrieval_output.result,
        };

        let think_output = pipeline_runner.run(&self.think, think_input).await?;

        let generate_input = GenerateInput {
            query: think_output.result,
        };

        let generate_output = pipeline_runner.run(&self.generate, generate_input).await?;

        let validate_input = ValidateInput {
            query: generate_output.result,
        };

        let mut validate_output = pipeline_runner.run(&self.validate, validate_input).await?;
        let mut cached_validate_result = validate_output.result;

        while !validate_output.is_valid {
            let repair_input = RepairInput {
                query: cached_validate_result,
                scores: validate_output.scores,
            };

            let repair_output = pipeline_runner.run(&self.repair, repair_input).await?;

            let validate_input = ValidateInput {
                query: repair_output.result,
            };

            validate_output = pipeline_runner.run(&self.validate, validate_input).await?;
            cached_validate_result = validate_output.result;
        }

        let extract_input = ExtractInput {
            query: cached_validate_result,
        };

        let extract_output = pipeline_runner.run(&self.extract, extract_input).await?;

        let commit_input = CommitInput {
            query: extract_output.result,
        };

        let commit_output = pipeline_runner.run(&self.commit, commit_input).await?;

        Ok(TurnResult {
            result: CommittedTurnInfo {
                turn_number: 1,
                story_revision: 1,
                story_text: commit_output.result,
            },
            replayed: false,
        })
    }
}
