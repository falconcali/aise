use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ValidationResult {
    pub pass: bool,
    pub issues: Vec<ValidationIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub severity: Severity,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Fatal,
    Warning,
}

pub fn fatal(code: &str, message: impl Into<String>) -> ValidationIssue {
    ValidationIssue {
        severity: Severity::Fatal,
        code: code.into(),
        message: message.into(),
    }
}
