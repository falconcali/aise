use crate::core::{
    ActivatedKnowledge, BoundedText, CharacterDecision, FactEntry, NarrativeDirection, PlayerContribution,
    PlayerRoleContext, ResolvedRole, RumorEntry,
};
use serde::Serialize;

#[derive(Clone, Copy, Serialize)]
pub struct ValidationContext<'a> {
    pub story_goal: &'a BoundedText,
    pub player_contribution: &'a PlayerContribution,
    pub directions: &'a [NarrativeDirection],
    pub constraints: &'a [BoundedText],
    pub knowledge: &'a ActivatedKnowledge,
    pub retrieved_facts: &'a [FactEntry],
    pub retrieved_rumors: &'a [RumorEntry],
    pub player_role: &'a PlayerRoleContext,
    pub roles: &'a [ResolvedRole],
    pub character_decisions: &'a [CharacterDecision],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ValidationCriterionKind {
    Check,
    Score,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidationIssue {
    pub kind: ValidationCriterionKind,
    pub name: String,
    pub message: BoundedText,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidationCheckResult {
    pub name: String,
    pub description: String,
    pub passed: bool,
    pub issue: Option<BoundedText>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ValidationScoreResult {
    pub name: String,
    pub description: String,
    pub min_score: f64,
    pub max_score: f64,
    pub target_score: f64,
    pub score: f64,
    pub issue: Option<BoundedText>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ValidationReport {
    pub proposal_version: u32,
    pub checks: Vec<ValidationCheckResult>,
    pub scores: Vec<ValidationScoreResult>,
}

impl ValidationReport {
    pub fn all_checks_pass(&self) -> bool {
        self.checks.iter().all(|check| check.passed)
    }

    pub fn all_scores_meet_target(&self) -> bool {
        self.scores.iter().all(|score| score.score >= score.target_score)
    }

    pub fn failed_issues(&self) -> Vec<ValidationIssue> {
        self.checks
            .iter()
            .filter(|check| !check.passed)
            .map(|check| ValidationIssue {
                kind: ValidationCriterionKind::Check,
                name: check.name.clone(),
                message: check.issue.clone().expect("failed validation check must include an issue"),
            })
            .chain(
                self.scores
                    .iter()
                    .filter(|score| score.score < score.target_score)
                    .map(|score| ValidationIssue {
                        kind: ValidationCriterionKind::Score,
                        name: score.name.clone(),
                        message: score.issue.clone().expect("unmet validation score must include an issue"),
                    }),
            )
            .collect()
    }

    pub fn unmet_score_count(&self) -> usize {
        self.scores.iter().filter(|score| score.score < score.target_score).count()
    }
}

#[cfg(test)]
#[path = "tests/validation_tests.rs"]
mod tests;
