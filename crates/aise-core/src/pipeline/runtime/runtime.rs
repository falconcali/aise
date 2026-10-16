use crate::core::{
    RoleId, StoryCommit, StoryContext, TurnCheckEvaluation, TurnChoiceEvaluation, TurnControl, TurnEvaluation,
    TurnEventSink, TurnRequest, TurnScoreEvaluation, TurnSegment, TurnStatus, WorldChange,
};
use crate::llm::LlmGateway;
use crate::persistence::Store;
use crate::pipeline::baseline::{BaselineInput, BaselineOutput, BaselinePipeline};
use crate::pipeline::commit::{CommitInput, CommitPipeline};
use crate::pipeline::common::{
    PipelineError, PipelineRunner, PipelineStage, ValidationContext, ValidationCriterionKind, ValidationIssue,
    ValidationReport,
};
use crate::pipeline::extract::{ExtractInput, ExtractPipeline};
use crate::pipeline::generate::{GenerateInput, GeneratePipeline};
use crate::pipeline::plan::{PlanInput, PlanOutput, PlanPipeline};
use crate::pipeline::repair::{RepairInput, RepairOutput, RepairPipeline};
use crate::pipeline::retrieval::{RetrievalInput, RetrievalOutput, RetrievalPipeline};
use crate::pipeline::runtime::validation_choice::select_choice;
use crate::pipeline::summary::{SummaryInput, SummaryPipeline};
use crate::pipeline::think::{ThinkInput, ThinkOutput, ThinkPipeline};
use crate::pipeline::validate::{ValidateInput, ValidateOutput, ValidatePipeline};
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
    validate: ValidatePipeline,
    repair: RepairPipeline,
    extract: ExtractPipeline,
    summary: SummaryPipeline,
    commit: CommitPipeline,
    validation_loop: ValidationLoopConfig,
    repair_max_proposal_bytes: usize,
}

pub(super) struct ValidatedProposal {
    pub(super) proposal: String,
    pub(super) report: ValidationReport,
}

struct ValidationLoopResult {
    proposal: String,
    report: ValidationReport,
    repair_rounds: u32,
    status: TurnStatus,
    choice: Option<TurnChoiceEvaluation>,
}

impl Runtime {
    pub fn new(
        gateway: Arc<LlmGateway>,
        prompt: Arc<Prompt>,
        store: Arc<Store>,
        pipeline_config: PipelineConfig,
    ) -> Result<Self, PipelineError> {
        pipeline_config.validate()?;
        let repair_max_proposal_bytes = pipeline_config.repair.max_proposal_bytes;
        Ok(Self {
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
            think: ThinkPipeline::new(
                Arc::clone(&gateway),
                Arc::clone(&prompt),
                Arc::clone(&store),
                pipeline_config.think,
            ),
            generate: GeneratePipeline::new(
                Arc::clone(&gateway),
                Arc::clone(&prompt),
                Arc::clone(&store),
                pipeline_config.generate,
            ),
            validate: ValidatePipeline::new(
                Arc::clone(&gateway),
                Arc::clone(&prompt),
                Arc::clone(&store),
                pipeline_config.validate,
            ),
            repair: RepairPipeline::new(
                Arc::clone(&gateway),
                Arc::clone(&prompt),
                Arc::clone(&store),
                pipeline_config.repair,
            ),
            extract: ExtractPipeline::new(Arc::clone(&store)),
            summary: SummaryPipeline::new(
                Arc::clone(&gateway),
                Arc::clone(&prompt),
                Arc::clone(&store),
                pipeline_config.summary,
            ),
            commit: CommitPipeline::new(Arc::clone(&store)),
            validation_loop: pipeline_config.validation_loop,
            repair_max_proposal_bytes,
        })
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
                role_ids: &role_ids,
            };
            pipeline_runner.run(&self.retrieval, retrieval_input, story_ctx).await?
        } else {
            RetrievalOutput::default()
        };

        let think_output = if plan_output.requires_character_thinking() {
            let think_input = ThinkInput {
                requests: &plan_output.character_think_requests,
                resolved_roles: &retrieval_output.roles,
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
            player_role: &plan_output.player_role,
            roles: &retrieval_output.roles,
            character_decisions: &think_output.decisions,
        };

        let generate_output = pipeline_runner.run(&self.generate, generate_input, story_ctx).await?;

        let initial_proposal = validate_generated_proposal(generate_output.result, self.repair_max_proposal_bytes)?;
        let validation_result = self
            .validate_and_repair(
                &pipeline_runner,
                story_ctx,
                validation_context(&plan_output, &baseline_output, &retrieval_output, &think_output),
                initial_proposal,
            )
            .await?;

        let extract_input = ExtractInput {
            query: &validation_result.proposal,
            player_contribution: &baseline_output.player_contribution,
            queries: &baseline_output.narrative.queries,
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
            world_change: WorldChange {},
            turn_status: validation_result.status,
            turn_evaluation,
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
        context: ValidationContext<'_>,
        initial_proposal: String,
    ) -> Result<ValidationLoopResult, PipelineError> {
        let mut history = Vec::with_capacity(self.validation_loop.max_repair_rounds as usize + 1);
        let mut current_proposal = initial_proposal;
        let mut repair_rounds = 0;

        loop {
            let proposal_version =
                u32::try_from(history.len() + 1).expect("validated proposal history length must fit u32");
            let validate_input = ValidateInput {
                context,
                proposal: &current_proposal,
                proposal_version,
                repair_rounds_used: repair_rounds,
                repair_rounds_remaining: self.validation_loop.max_repair_rounds - repair_rounds,
            };

            let validate_output: ValidateOutput =
                pipeline_runner.run(&self.validate, validate_input, story_ctx).await?;
            history.push(ValidatedProposal {
                proposal: current_proposal,
                report: validate_output.report,
            });
            let latest = history.last().expect("validated proposal history must not be empty");
            if latest.report.all_checks_pass() && latest.report.all_scores_meet_target() {
                let passed = history.pop().expect("validated proposal history must not be empty");
                return Ok(ValidationLoopResult {
                    proposal: passed.proposal,
                    report: passed.report,
                    repair_rounds,
                    status: TurnStatus::Pass,
                    choice: None,
                });
            }

            if repair_rounds >= self.validation_loop.max_repair_rounds {
                if !latest.report.all_checks_pass() {
                    let issues = latest.report.failed_issues();
                    let (failed_checks, unmet_scores) = failed_issue_counts(&issues);
                    return Err(PipelineError::ValidationFailed {
                        code: "validation_hard_gate_exhausted",
                        proposal_version: latest.report.proposal_version,
                        message: format!(
                            "repair budget exhausted with {failed_checks} failed checks and {unmet_scores} unmet scores"
                        ),
                    });
                }
                let selection = select_choice(&history)?;
                let selected = history.swap_remove(selection.history_index);
                return Ok(ValidationLoopResult {
                    proposal: selected.proposal,
                    choice: Some(TurnChoiceEvaluation {
                        selected_proposal_version: selection.proposal_version,
                        target_gap_mse: selection.target_gap_mse,
                    }),
                    report: selected.report,
                    repair_rounds,
                    status: TurnStatus::Choice,
                });
            }

            repair_rounds += 1;
            let latest = history.last().expect("validated proposal history must not be empty");
            let issues = latest.report.failed_issues();
            let repair_input = RepairInput {
                context,
                proposal: &latest.proposal,
                proposal_version: latest.report.proposal_version,
                issues: &issues,
                repair_round: repair_rounds,
                repair_rounds_remaining: self.validation_loop.max_repair_rounds - repair_rounds,
            };

            let repair_output: RepairOutput = pipeline_runner.run(&self.repair, repair_input, story_ctx).await?;
            current_proposal = repair_output.proposal;
        }
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

fn validation_context<'a>(
    plan_output: &'a PlanOutput,
    baseline_output: &'a BaselineOutput,
    retrieval_output: &'a RetrievalOutput,
    think_output: &'a ThinkOutput,
) -> ValidationContext<'a> {
    ValidationContext {
        story_goal: &plan_output.story_goal,
        player_contribution: &baseline_output.player_contribution,
        directions: &baseline_output.narrative.directions,
        constraints: &baseline_output.narrative.constraints,
        knowledge: &baseline_output.knowledge,
        retrieved_facts: &retrieval_output.facts,
        retrieved_rumors: &retrieval_output.rumors,
        player_role: &plan_output.player_role,
        roles: &retrieval_output.roles,
        character_decisions: &think_output.decisions,
    }
}

fn turn_evaluation(result: &ValidationLoopResult) -> TurnEvaluation {
    TurnEvaluation {
        proposal_version: result.report.proposal_version,
        repair_rounds: result.repair_rounds,
        checks: result
            .report
            .checks
            .iter()
            .map(|check| TurnCheckEvaluation {
                name: check.name.clone(),
                passed: check.passed,
                issue: check.issue.clone(),
            })
            .collect(),
        scores: result
            .report
            .scores
            .iter()
            .map(|score| TurnScoreEvaluation {
                name: score.name.clone(),
                min_score: score.min_score,
                max_score: score.max_score,
                target_score: score.target_score,
                score: score.score,
                issue: score.issue.clone(),
            })
            .collect(),
        choice: result.choice.clone(),
    }
}

fn validate_generated_proposal(proposal: String, max_proposal_bytes: usize) -> Result<String, PipelineError> {
    if proposal.trim().is_empty() {
        return Err(PipelineError::EmptyOutput {
            stage: PipelineStage::Generate,
        });
    }
    if proposal.len() > max_proposal_bytes {
        return Err(PipelineError::invalid_output(
            PipelineStage::Generate,
            "generate_proposal_too_long",
            format!("generated proposal exceeds {max_proposal_bytes} bytes"),
        ));
    }
    Ok(proposal)
}

fn failed_issue_counts(issues: &[ValidationIssue]) -> (usize, usize) {
    issues.iter().fold((0, 0), |(checks, scores), issue| match issue.kind {
        ValidationCriterionKind::Check => (checks + 1, scores),
        ValidationCriterionKind::Score => (checks, scores + 1),
    })
}

#[cfg(test)]
#[path = "tests/runtime_tests.rs"]
mod tests;
