use crate::core::{
    StoryCommit, StoryContext, TurnControl, TurnEvaluation, TurnEventSink, TurnRequest, TurnSegment, WorldChange,
};
use crate::llm::LlmGateway;
use crate::persistence::StoryStore;
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
    pub fn new(gateway: Arc<LlmGateway>, prompt: Arc<Prompt>, story_store: Arc<dyn StoryStore>) -> Self {
        Self {
            baseline: BaselinePipeline::new(Arc::clone(&gateway), Arc::clone(&prompt), Arc::clone(&story_store)),
            plan: PlanPipeline::new(Arc::clone(&gateway), Arc::clone(&prompt), Arc::clone(&story_store)),
            retrieval: RetrievalPipeline,
            think: ThinkPipeline,
            generate: GeneratePipeline::new(Arc::clone(&gateway), Arc::clone(&prompt), Arc::clone(&story_store)),
            validate: ValidatePipeline {
                score_configs: vec![],
                validation_budget: 10,
            },
            repair: RepairPipeline,
            extract: ExtractPipeline,
            commit: CommitPipeline::new(Arc::clone(&story_store)),
        }
    }

    pub async fn run_turn(
        &self,
        story_ctx: &StoryContext,
        turn_request: TurnRequest,
        turn_control: TurnControl,
        sink: &dyn TurnEventSink,
        trace: &Trace,
    ) -> Result<StoryCommit, PipelineError> {
        let pipeline_runner = PipelineRunner {
            control: &turn_control,
            sink,
            trace,
        };

        let baseline_input = BaselineInput {
            player_input: turn_request.player_input.clone(),
        };

        let baseline_output = pipeline_runner.run(&self.baseline, baseline_input, story_ctx).await?;

        let plan_input = PlanInput {
            player_contribution: baseline_output.player_contribution.clone(),
        };

        let plan_output = pipeline_runner.run(&self.plan, plan_input, story_ctx).await?;
        let mut generate_input = GenerateInput {
            story_goal: plan_output.plan.clone(),
            player_contribution: plan_output.player_contribution.clone(),
        };

        if plan_output.requires_retrieval() {
            let retrieval_input = RetrievalInput {
                query: plan_output.plan.clone(),
            };

            let retrieval_output = pipeline_runner.run(&self.retrieval, retrieval_input, story_ctx).await?;
        }

        if plan_output.requires_character_thinking() {
            let think_input = ThinkInput {
                query: plan_output.plan.clone(),
            };

            let think_output = pipeline_runner.run(&self.think, think_input, story_ctx).await?;
        }

        let generate_output = pipeline_runner.run(&self.generate, generate_input, story_ctx).await?;

        let validate_input = ValidateInput {
            original_proposal: generate_output.result.clone(),
            current_proposal: generate_output.result,
            proposal_version: 1,
            player_contribution: generate_output.player_contribution,
        };

        let mut validate_output = pipeline_runner.run(&self.validate, validate_input, story_ctx).await?;
        let mut validate_history = vec![validate_output.clone()];

        while validate_output.decision == ValidationDecision::Repair {
            let repair_input = RepairInput {
                original_proposal: validate_output.original_proposal,
                current_proposal: validate_output.current_proposal,
                scores: validate_output.scores,
                proposal_version: validate_output.proposal_version,
                player_contribution: validate_output.player_contribution,
            };

            let repair_output = pipeline_runner.run(&self.repair, repair_input, story_ctx).await?;

            let validate_input = ValidateInput {
                original_proposal: repair_output.original_proposal,
                current_proposal: repair_output.current_proposal,
                proposal_version: repair_output.proposal_version,
                player_contribution: repair_output.player_contribution,
            };

            validate_output = pipeline_runner.run(&self.validate, validate_input, story_ctx).await?;
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
            player_contribution: validate_output.player_contribution,
        };

        let extract_output = pipeline_runner.run(&self.extract, extract_input, story_ctx).await?;

        let commit_input: CommitInput = CommitInput {
            player_contribution: extract_output.player_contribution,
            idempotency_key: turn_request.idempotency_key,
            turn_segment: TurnSegment::new(extract_output.result),
            world_change: WorldChange {},
            turn_evaluation: TurnEvaluation {},
        };

        pipeline_runner.run(&self.commit, commit_input, story_ctx).await
    }
}
