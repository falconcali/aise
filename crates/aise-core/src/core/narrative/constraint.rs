use crate::core::BoundedText;
use crate::core::narrative::NodeStatus;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintWindow {
    BeforeActive,
    WhileActive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NarrativeConstraint {
    pub forbid: BoundedText,
    pub window: ConstraintWindow,
}

impl NarrativeConstraint {
    pub fn applies(&self, status: NodeStatus) -> bool {
        matches!(
            (self.window, status),
            (ConstraintWindow::BeforeActive, NodeStatus::Inactive)
                | (ConstraintWindow::WhileActive, NodeStatus::Active)
        )
    }
}

#[cfg(test)]
#[path = "tests/constraint_tests.rs"]
mod tests;
