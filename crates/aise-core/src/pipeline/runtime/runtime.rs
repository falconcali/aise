use crate::core::{
    BoundedText, PlayerContribution, StoryCommit, StoryContext, TurnControl, TurnEvaluation, TurnEventSink,
    TurnRequest, TurnSegment, WorldChange,
};
use crate::llm::LlmGateway;
use crate::persistence::Store;
use crate::pipeline::PipelineConfig;
use crate::pipeline::baseline::{BaselineInput, BaselinePipeline};
use crate::pipeline::commit::{CommitInput, CommitPipeline};
use crate::pipeline::common::{PipelineError, PipelineRunner};
use crate::pipeline::extract::{ExtractInput, ExtractPipeline};
use crate::pipeline::generate::{GenerateInput, GeneratePipeline};
use crate::pipeline::plan::{PlanInput, PlanOutput, PlanPipeline};
use crate::pipeline::repair::{RepairInput, RepairPipeline};
use crate::pipeline::retrieval::{RetrievalInput, RetrievalOutput, RetrievalPipeline};
use crate::pipeline::summary::{SummaryInput, SummaryPipeline};
use crate::pipeline::think::{ThinkInput, ThinkPipeline};
use crate::pipeline::validate::{ValidateInput, ValidateOutput, ValidatePipeline, ValidationDecision};
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
    summary: SummaryPipeline,
    commit: CommitPipeline,
}

impl Runtime {
    pub fn new(
        gateway: Arc<LlmGateway>,
        prompt: Arc<Prompt>,
        store: Arc<Store>,
        pipeline_config: PipelineConfig,
    ) -> Self {
        Self {
            baseline: BaselinePipeline::new(
                Arc::clone(&gateway),
                Arc::clone(&prompt),
                Arc::clone(&store),
                pipeline_config.baseline,
            ),
            plan: PlanPipeline::new(
                Arc::clone(&gateway),
                Arc::clone(&prompt),
                Arc::clone(&store),
                pipeline_config.plan,
            ),
            retrieval: RetrievalPipeline::new(Arc::clone(&store)),
            think: ThinkPipeline,
            generate: GeneratePipeline::new(Arc::clone(&gateway), Arc::clone(&prompt), Arc::clone(&store)),
            validate: ValidatePipeline { score_configs: vec![] },
            repair: RepairPipeline,
            extract: ExtractPipeline::new(Arc::clone(&store)),
            summary: SummaryPipeline::new(
                Arc::clone(&gateway),
                Arc::clone(&prompt),
                Arc::clone(&store),
                pipeline_config.summary,
            ),
            commit: CommitPipeline::new(Arc::clone(&store)),
        }
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

        let retrieval_output = if plan_output.requires_retrieval() {
            let retrieval_input = RetrievalInput {
                fact_ids: &plan_output.facts_to_retrieve,
                rumor_ids: &plan_output.rumors_to_retrieve,
            };
            pipeline_runner.run(&self.retrieval, retrieval_input, story_ctx).await?
        } else {
            RetrievalOutput::default()
        };

        if plan_output.requires_character_thinking() {
            let think_input = ThinkInput {
                query: &plan_output.story_goal,
            };

            let _think_output = pipeline_runner.run(&self.think, think_input, story_ctx).await?;
        }

        let generate_input = GenerateInput {
            story_goal: &plan_output.story_goal,
            player_contribution: &baseline_output.player_contribution,
            directions: &baseline_output.narrative.directions,
            constraints: &baseline_output.narrative.constraints,
            knowledge: &baseline_output.knowledge,
            retrieved_facts: &retrieval_output.facts,
            retrieved_rumors: &retrieval_output.rumors,
        };

        let generate_output = pipeline_runner.run(&self.generate, generate_input, story_ctx).await?;

        let final_proposal = self
            .validate_and_repair(
                &pipeline_runner,
                story_ctx,
                generate_output.result,
                &baseline_output.narrative.constraints,
                &baseline_output.player_contribution,
            )
            .await?;

        let extract_input = ExtractInput {
            query: &final_proposal,
            player_contribution: &baseline_output.player_contribution,
            queries: &baseline_output.narrative.queries,
        };

        let extract_output = pipeline_runner.run(&self.extract, extract_input, story_ctx).await?;

        let summary_input = SummaryInput {
            pending_turn_number: story_ctx.turn_number.increment(),
        };

        let summary_output = pipeline_runner.run(&self.summary, summary_input, story_ctx).await?;

        let commit_input: CommitInput = CommitInput {
            player_contribution: baseline_output.player_contribution,
            idempotency_key: turn_request.idempotency_key,
            turn_segment: TurnSegment::new(final_proposal),
            world_change: WorldChange {},
            turn_evaluation: TurnEvaluation {},
            summary: summary_output.summary,
            narrative: extract_output.narrative,
            life_cycle: extract_output.life_cycle,
        };

        pipeline_runner.run(&self.commit, commit_input, story_ctx).await
    }

    async fn validate_and_repair(
        &self,
        pipeline_runner: &PipelineRunner<'_>,
        story_ctx: &StoryContext,
        initial_proposal: String,
        constraints: &[BoundedText],
        player_contribution: &PlayerContribution,
    ) -> Result<String, PipelineError> {
        let mut proposals = vec![initial_proposal];
        let mut validations: Vec<ValidateOutput> = Vec::new();

        loop {
            let current = proposals.len() - 1;
            let validate_input = ValidateInput {
                original_proposal: &proposals[0],
                current_proposal: &proposals[current],
                proposal_version: proposal_version(&proposals),
                constraints,
                player_contribution,
            };

            let validate_output = pipeline_runner.run(&self.validate, validate_input, story_ctx).await?;
            let decision = validate_output.decision;
            validations.push(validate_output);

            if decision != ValidationDecision::Repair {
                break;
            }

            let repair_input = RepairInput {
                original_proposal: &proposals[0],
                current_proposal: &proposals[current],
                scores: &validations[current].scores,
                proposal_version: proposal_version(&proposals),
                constraints,
                player_contribution,
            };

            let repair_output = pipeline_runner.run(&self.repair, repair_input, story_ctx).await?;
            proposals.push(repair_output.current_proposal);
        }

        let final_index = select_final_index(&validations);
        Ok(proposals.swap_remove(final_index))
    }
}

fn proposal_version(proposals: &[String]) -> u32 {
    u32::try_from(proposals.len()).unwrap_or(u32::MAX)
}

fn select_final_index(validations: &[ValidateOutput]) -> usize {
    let last = validations.len().saturating_sub(1);
    if validations.get(last).map(|validation| validation.decision) != Some(ValidationDecision::Reject) {
        return last;
    }

    validations
        .iter()
        .enumerate()
        .max_by(|(_, left), (_, right)| left.total_score().total_cmp(&right.total_score()))
        .map_or(last, |(index, _)| index)
}

#[cfg(test)]
#[path = "tests/runtime_tests.rs"]
mod tests;
