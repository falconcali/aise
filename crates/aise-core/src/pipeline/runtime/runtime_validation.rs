use crate::core::{
    RoleId, StoryContext, TurnCheckEvaluation, TurnChoiceEvaluation, TurnEvaluation, TurnScoreEvaluation, TurnStatus,
};
use crate::pipeline::baseline::BaselineOutput;
use crate::pipeline::common::{
    PipelineError, PipelineRunner, PipelineStage, ValidationContext, ValidationIssue, ValidationReport,
};
use crate::pipeline::plan::PlanOutput;
use crate::pipeline::repair::{RepairInput, RepairOutput};
use crate::pipeline::retrieval::RetrievalOutput;
use crate::pipeline::runtime::runtime::Runtime;
use crate::pipeline::runtime::runtime_trace;
use crate::pipeline::think::ThinkOutput;
use crate::pipeline::validate::{ValidateInput, ValidateOutput};
use std::cmp::Ordering;

pub(super) struct ValidatedProposal {
    pub(super) proposal: String,
    pub(super) report: ValidationReport,
}

pub(super) struct ValidationLoopResult {
    pub(super) proposal: String,
    pub(super) report: ValidationReport,
    pub(super) repair_rounds: u32,
    pub(super) status: TurnStatus,
    pub(super) choice: Option<TurnChoiceEvaluation>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ChoiceSelection {
    pub history_index: usize,
    pub proposal_version: u32,
    pub target_gap_mse: f64,
    pub unmet_score_count: usize,
}

impl Runtime {
    pub(super) async fn validate_repair_loop(
        &self,
        pipeline_runner: &PipelineRunner<'_>,
        story_ctx: &StoryContext,
        context: ValidationContext<'_>,
        initial_proposal: String,
    ) -> Result<ValidationLoopResult, PipelineError> {
        let validation_observation = runtime_trace::begin_validation_loop(pipeline_runner.trace, &initial_proposal);
        let result = self
            .validate_and_repair(pipeline_runner, story_ctx, context, initial_proposal)
            .await;
        runtime_trace::finish_validation_loop(validation_observation, &result);
        result
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

pub(super) fn validation_context<'a>(
    plan_output: &'a PlanOutput,
    baseline_output: &'a BaselineOutput,
    retrieval_output: &'a RetrievalOutput,
    role_ids: &'a [RoleId],
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
        role_ids,
        character_decisions: &think_output.decisions,
    }
}

pub(super) fn turn_evaluation(result: &ValidationLoopResult) -> TurnEvaluation {
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

pub(super) fn validate_generated_proposal(
    proposal: String,
    max_proposal_bytes: usize,
) -> Result<String, PipelineError> {
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

pub(super) fn select_choice(history: &[ValidatedProposal]) -> Result<ChoiceSelection, PipelineError> {
    let mut best = None;
    for (history_index, proposal) in history.iter().enumerate() {
        if !proposal.report.all_checks_pass() {
            continue;
        }
        let candidate = ChoiceSelection {
            history_index,
            proposal_version: proposal.report.proposal_version,
            target_gap_mse: target_gap_mse(proposal)?,
            unmet_score_count: proposal.report.unmet_score_count(),
        };
        if best.as_ref().is_none_or(|current| is_better(candidate, *current)) {
            best = Some(candidate);
        }
    }
    best.ok_or_else(|| PipelineError::ValidationFailed {
        code: "validation_choice_unavailable",
        proposal_version: history.last().map(|entry| entry.report.proposal_version).unwrap_or(0),
        message: "no hard-gate-passing proposal is available for choice".to_owned(),
    })
}

pub(super) fn target_gap_mse(proposal: &ValidatedProposal) -> Result<f64, PipelineError> {
    if proposal.report.scores.is_empty() {
        return Err(PipelineError::ValidationFailed {
            code: "validation_choice_scores_empty",
            proposal_version: proposal.report.proposal_version,
            message: "choice candidate has no validation scores".to_owned(),
        });
    }
    let metric = proposal
        .report
        .scores
        .iter()
        .map(|score| {
            let deficit = ((score.target_score - score.score) / (score.max_score - score.min_score)).max(0.0);
            deficit * deficit
        })
        .sum::<f64>()
        / proposal.report.scores.len() as f64;
    if !metric.is_finite() {
        return Err(PipelineError::ValidationFailed {
            code: "validation_choice_score_non_finite",
            proposal_version: proposal.report.proposal_version,
            message: "choice candidate produced a non-finite target gap metric".to_owned(),
        });
    }
    Ok(metric)
}

fn is_better(candidate: ChoiceSelection, current: ChoiceSelection) -> bool {
    match candidate.target_gap_mse.total_cmp(&current.target_gap_mse) {
        Ordering::Less => true,
        Ordering::Greater => false,
        Ordering::Equal => match candidate.unmet_score_count.cmp(&current.unmet_score_count) {
            Ordering::Less => true,
            Ordering::Greater => false,
            Ordering::Equal => candidate.proposal_version > current.proposal_version,
        },
    }
}

fn failed_issue_counts(issues: &[ValidationIssue]) -> (usize, usize) {
    issues.iter().fold((0, 0), |(checks, scores), issue| match issue {
        ValidationIssue::Check { .. } => (checks + 1, scores),
        ValidationIssue::Score { .. } => (checks, scores + 1),
    })
}

#[cfg(test)]
#[path = "tests/runtime_validation_tests.rs"]
mod tests;
