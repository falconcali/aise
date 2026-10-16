use crate::pipeline::common::PipelineError;
use crate::pipeline::runtime::runtime::ValidatedProposal;
use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ChoiceSelection {
    pub history_index: usize,
    pub proposal_version: u32,
    pub target_gap_mse: f64,
    pub unmet_score_count: usize,
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

#[cfg(test)]
#[path = "tests/validation_choice_tests.rs"]
mod tests;
