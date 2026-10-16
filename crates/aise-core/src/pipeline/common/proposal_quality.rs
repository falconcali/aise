use crate::core::BoundedText;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProposalCheckCriterion {
    pub name: String,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProposalScoreCriterion {
    pub name: String,
    pub description: String,
    pub min_score: f64,
    pub max_score: f64,
    pub target_score: f64,
    pub target_guidance: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ProposalQualityPolicy {
    checks: Box<[ProposalCheckCriterion]>,
    scores: Box<[ProposalScoreCriterion]>,
}

impl ProposalQualityPolicy {
    pub fn new(checks: Vec<ProposalCheckCriterion>, scores: Vec<ProposalScoreCriterion>) -> Self {
        Self {
            checks: checks.into_boxed_slice(),
            scores: scores.into_boxed_slice(),
        }
    }

    pub fn checks(&self) -> &[ProposalCheckCriterion] {
        &self.checks
    }

    pub fn scores(&self) -> &[ProposalScoreCriterion] {
        &self.scores
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind")]
pub enum ValidationIssue {
    Check {
        name: String,
        message: BoundedText,
    },
    Score {
        name: String,
        current_score: f64,
        target_score: f64,
        message: BoundedText,
    },
}

#[cfg(test)]
#[path = "tests/proposal_quality_tests.rs"]
mod tests;
