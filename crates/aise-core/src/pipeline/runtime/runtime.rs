use crate::core::{CommittedTurnInfo, StoryContext, TurnControl, TurnEventSink, TurnRequest, TurnResult};
use crate::llm::LlmGateway;
use crate::pipeline::baseline::{BaselineInput, BaselinePipeline};
use crate::pipeline::commit::{CommitInput, CommitPipeline};
use crate::pipeline::common::{PipelineError, PipelineRunner};
use crate::pipeline::extract::{ExtractInput, ExtractPipeline};
use crate::pipeline::generate::{GenerateInput, GeneratePipeline};
use crate::pipeline::plan::{PlanInput, PlanPipeline};
use crate::pipeline::repair::{RepairInput, RepairPipeline};
use crate::pipeline::retrieval::{RetrievalInput, RetrievalPipeline};
use crate::pipeline::think::{ThinkInput, ThinkPipeline};
use crate::pipeline::validate::{ValidateInput, ValidatePipeline, ValidationDecision};
use crate::prompt::Prompt;
use crate::trace::Trace;
use std::sync::Arc;

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

impl Runtime {
    pub fn new(gateway: Arc<LlmGateway>, prompt: Arc<Prompt>) -> Self {
        Self {
            baseline: BaselinePipeline::new(gateway, prompt),
            plan: PlanPipeline,
            retrieval: RetrievalPipeline,
            think: ThinkPipeline,
            generate: GeneratePipeline,
            validate: ValidatePipeline {
                score_configs: vec![],
                validation_budget: 10,
            },
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
            trace,
        };

        let baseline_input = BaselineInput {
            story_ctx: StoryContext {
                story_id: turn_request.story_id.clone(),
            },
            player_input: turn_request.player_input.clone(),
        };

        let baseline_output = pipeline_runner.run(&self.baseline, baseline_input).await?;

        let plan_input = PlanInput {
            story_ctx: StoryContext {
                story_id: turn_request.story_id.clone(),
            },
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
            original_proposal: generate_output.result.clone(),
            current_proposal: generate_output.result,
            proposal_version: 1,
        };

        let mut validate_output = pipeline_runner.run(&self.validate, validate_input).await?;
        let mut validate_history = vec![validate_output.clone()];

        while validate_output.decision == ValidationDecision::Repair {
            let repair_input = RepairInput {
                original_proposal: validate_output.original_proposal,
                current_proposal: validate_output.current_proposal,
                scores: validate_output.scores,
                proposal_version: validate_output.proposal_version,
            };

            let repair_output = pipeline_runner.run(&self.repair, repair_input).await?;

            let validate_input = ValidateInput {
                original_proposal: repair_output.original_proposal,
                current_proposal: repair_output.current_proposal,
                proposal_version: repair_output.proposal_version,
            };

            validate_output = pipeline_runner.run(&self.validate, validate_input).await?;
            validate_history.push(validate_output.clone());
        }

        if validate_output.decision == ValidationDecision::Reject {
            validate_output = validate_history
                .into_iter()
                .max_by(|left, right| {
                    let left_score = left.scores.iter().map(|score| score.score).sum::<f32>();
                    let right_score = right.scores.iter().map(|score| score.score).sum::<f32>();
                    left_score.total_cmp(&right_score)
                })
                .expect("validation history is initialized with the first validation result");

            validate_output.decision = ValidationDecision::Reject;
        }

        let extract_input = ExtractInput {
            query: validate_output.current_proposal,
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
