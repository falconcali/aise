use crate::core::{RoleId, StoryCommit, StoryContext, TurnControl, TurnEventSink, TurnRequest, TurnSegment};
use crate::llm::LlmGateway;
use crate::persistence::Store;
use crate::pipeline::baseline::{BaselineInput, BaselinePipeline};
use crate::pipeline::commit::{CommitInput, CommitPipeline};
use crate::pipeline::common::{PipelineError, PipelineRunner};
use crate::pipeline::extract::{ExtractInput, ExtractPipeline};
use crate::pipeline::generate::{GenerateInput, GeneratePipeline};
use crate::pipeline::plan::{PlanInput, PlanOutput, PlanPipeline};
use crate::pipeline::repair::RepairPipeline;
use crate::pipeline::retrieval::{RetrievalInput, RetrievalOutput, RetrievalPipeline};
use crate::pipeline::runtime::runtime_validation::{turn_evaluation, validate_generated_proposal, validation_context};
use crate::pipeline::summary::{SummaryInput, SummaryPipeline};
use crate::pipeline::think::{ThinkInput, ThinkOutput, ThinkPipeline};
use crate::pipeline::validate::ValidatePipeline;
use crate::pipeline::{PipelineConfig, ValidationLoopConfig};
use crate::prompt::Prompt;
use crate::trace::Trace;
use std::collections::HashSet;
use std::sync::Arc;

pub struct Runtime {
    baseline: BaselinePipeline,
    plan: PlanPipeline,
    retrieval: RetrievalPipeline,
    think: ThinkPipeline,
    generate: GeneratePipeline,
    pub(super) validate: ValidatePipeline,
    pub(super) repair: RepairPipeline,
    extract: ExtractPipeline,
    summary: SummaryPipeline,
    commit: CommitPipeline,
    pub(super) validation_loop: ValidationLoopConfig,
    repair_max_proposal_bytes: usize,
}

impl Runtime {
    pub fn new(
        gateway: Arc<LlmGateway>,
        prompt: Arc<Prompt>,
        store: Arc<Store>,
        pipeline_config: PipelineConfig,
    ) -> Result<Self, PipelineError> {
        pipeline_config.validate()?;
        let quality_policy = Arc::new(pipeline_config.validate.quality_policy());
        let repair_max_proposal_bytes = pipeline_config.repair.max_proposal_bytes;
        Ok(Self {
            baseline: BaselinePipeline::new(
                Arc::clone(&gateway),
                Arc::clone(&prompt),
                Arc::clone(&store),
                pipeline_config.baseline,
            ),
            plan: PlanPipeline::new(Arc::clone(&gateway), Arc::clone(&prompt), pipeline_config.plan),
            retrieval: RetrievalPipeline::new(),
            think: ThinkPipeline::new(Arc::clone(&gateway), Arc::clone(&prompt), pipeline_config.think),
            generate: GeneratePipeline::new(
                Arc::clone(&gateway),
                Arc::clone(&prompt),
                pipeline_config.generate,
                Arc::clone(&quality_policy),
            ),
            validate: ValidatePipeline::new(
                Arc::clone(&gateway),
                Arc::clone(&prompt),
                pipeline_config.validate,
                Arc::clone(&quality_policy),
            ),
            repair: RepairPipeline::new(
                Arc::clone(&gateway),
                Arc::clone(&prompt),
                pipeline_config.repair,
                Arc::clone(&quality_policy),
            ),
            extract: ExtractPipeline::new(
                Arc::clone(&gateway),
                Arc::clone(&prompt),
                Arc::clone(&store),
                pipeline_config.extract,
            ),
            summary: SummaryPipeline::new(Arc::clone(&gateway), Arc::clone(&prompt), pipeline_config.summary),
            commit: CommitPipeline::new(Arc::clone(&store)),
            validation_loop: pipeline_config.validation_loop,
            repair_max_proposal_bytes,
        })
    }

    #[cfg(test)]
    pub(super) fn stages_share_quality_policy(&self) -> bool {
        Arc::ptr_eq(self.generate.quality_policy(), self.validate.quality_policy())
            && Arc::ptr_eq(self.generate.quality_policy(), self.repair.quality_policy())
    }

    pub async fn run_turn(
        &self,
        story_ctx: &StoryContext,
        turn_request: TurnRequest,
        turn_control: &TurnControl,
        sink: &dyn TurnEventSink,
        trace: &Trace,
    ) -> Result<StoryCommit, PipelineError> {
        let pipeline_runner = PipelineRunner {
            control: turn_control,
            sink,
            trace,
        };

        let baseline_input = BaselineInput {
            player_input: &turn_request.player_input,
        };

        let baseline_output = pipeline_runner.run(&self.baseline, baseline_input, story_ctx).await?;

        let plan_input = PlanInput {
            player_contribution: &baseline_output.player_contribution,
            directions: &baseline_output.narrative.directions,
            constraints: &baseline_output.narrative.constraints,
            knowledge: &baseline_output.knowledge,
        };

        let plan_output: PlanOutput = pipeline_runner.run(&self.plan, plan_input, story_ctx).await?;

        let role_ids = effective_role_ids(&plan_output);
        let retrieval_output = if plan_output.requires_retrieval() {
            let retrieval_input = RetrievalInput {
                fact_ids: &plan_output.facts_to_retrieve,
                rumor_ids: &plan_output.rumors_to_retrieve,
            };
            pipeline_runner.run(&self.retrieval, retrieval_input, story_ctx).await?
        } else {
            RetrievalOutput::default()
        };

        let think_output = if plan_output.requires_character_thinking() {
            let think_input = ThinkInput {
                requests: &plan_output.character_think_requests,
                player_contribution: &baseline_output.player_contribution,
                activated_rumors: &baseline_output.knowledge.rumors,
                retrieved_rumors: &retrieval_output.rumors,
            };

            pipeline_runner.run(&self.think, think_input, story_ctx).await?
        } else {
            ThinkOutput::default()
        };

        let generate_input = GenerateInput {
            story_goal: &plan_output.story_goal,
            player_contribution: &baseline_output.player_contribution,
            directions: &baseline_output.narrative.directions,
            constraints: &baseline_output.narrative.constraints,
            knowledge: &baseline_output.knowledge,
            retrieved_facts: &retrieval_output.facts,
            retrieved_rumors: &retrieval_output.rumors,
            role_ids: &role_ids,
            character_decisions: &think_output.decisions,
        };

        let generate_output = pipeline_runner.run(&self.generate, generate_input, story_ctx).await?;
        let initial_proposal = validate_generated_proposal(generate_output.result, self.repair_max_proposal_bytes)?;
        let validation_result = self
            .validate_repair_loop(
                &pipeline_runner,
                story_ctx,
                validation_context(&plan_output, &baseline_output, &retrieval_output, &role_ids, &think_output),
                initial_proposal,
            )
            .await?;

        let extract_input = ExtractInput {
            proposal: &validation_result.proposal,
        };

        let extract_output = pipeline_runner.run(&self.extract, extract_input, story_ctx).await?;

        let summary_input = SummaryInput {
            pending_turn_number: story_ctx.turn_number.increment(),
        };

        let summary_output = pipeline_runner.run(&self.summary, summary_input, story_ctx).await?;
        let turn_evaluation = turn_evaluation(&validation_result);

        let commit_input: CommitInput = CommitInput {
            player_contribution: baseline_output.player_contribution,
            idempotency_key: turn_request.idempotency_key,
            turn_segment: TurnSegment::new(validation_result.proposal),
            knowledge_change: extract_output.knowledge_change,
            turn_status: validation_result.status,
            turn_evaluation,
            summary: summary_output.summary,
            narrative: extract_output.narrative,
            life_cycle: extract_output.life_cycle,
        };

        pipeline_runner.run(&self.commit, commit_input, story_ctx).await
    }
}

fn effective_role_ids(plan_output: &PlanOutput) -> Vec<RoleId> {
    let mut seen =
        HashSet::with_capacity(plan_output.roles_for_generation.len() + plan_output.character_think_requests.len());
    let mut role_ids = Vec::with_capacity(seen.capacity());
    for role_id in plan_output
        .roles_for_generation
        .iter()
        .chain(plan_output.character_think_requests.iter().map(|request| &request.role_id))
    {
        if seen.insert(role_id.clone()) {
            role_ids.push(role_id.clone());
        }
    }
    role_ids
}

#[cfg(test)]
#[path = "tests/runtime_tests.rs"]
mod tests;
