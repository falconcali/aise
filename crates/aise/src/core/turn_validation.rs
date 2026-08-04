use crate::domain::character::CharacterState;
use crate::domain::memory::MemoryEntry;
use crate::domain::narrative::StoryEvent;
use crate::domain::world::WorldState;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationDecision {
    Pass,
    Repair,
    Reject,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    decision: ValidationDecision,
    issues: Vec<ValidationIssue>,
}

impl ValidationResult {
    pub fn pass() -> Self {
        Self {
            decision: ValidationDecision::Pass,
            issues: Vec::new(),
        }
    }

    pub fn repair(code: &str, message: impl Into<String>) -> Self {
        Self {
            decision: ValidationDecision::Repair,
            issues: vec![repairable(code, message)],
        }
    }

    pub fn reject(code: &str, message: impl Into<String>) -> Self {
        Self {
            decision: ValidationDecision::Reject,
            issues: vec![fatal(code, message)],
        }
    }

    pub fn with_issue(mut self, issue: ValidationIssue) -> Self {
        self.issues.push(issue);
        self
    }

    pub fn decision(&self) -> ValidationDecision {
        self.decision
    }

    pub fn issues(&self) -> &[ValidationIssue] {
        &self.issues
    }

    pub fn is_pass(&self) -> bool {
        self.decision == ValidationDecision::Pass
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    pub repairable: bool,
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
        repairable: false,
    }
}

pub fn repairable(code: &str, message: impl Into<String>) -> ValidationIssue {
    ValidationIssue {
        severity: Severity::Warning,
        code: code.into(),
        message: message.into(),
        repairable: true,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateChange<T> {
    Unchanged,
    Replace(T),
}

#[derive(Debug, Clone)]
pub struct ValidatedChangeSet {
    story_text: String,
    events: Vec<StoryEvent>,
    character_changes: Vec<CharacterState>,
    world_change: StateChange<WorldState>,
    memory_changes: Vec<MemoryEntry>,
    summary_delta: Option<String>,
}

impl ValidatedChangeSet {
    pub fn new(
        story_text: String,
        events: Vec<StoryEvent>,
        character_changes: Vec<CharacterState>,
        world_change: StateChange<WorldState>,
        memory_changes: Vec<MemoryEntry>,
        summary_delta: Option<String>,
    ) -> Self {
        Self {
            story_text,
            events,
            character_changes,
            world_change,
            memory_changes,
            summary_delta,
        }
    }

    pub fn story_text(&self) -> &str {
        &self.story_text
    }

    pub fn events(&self) -> &[StoryEvent] {
        &self.events
    }

    pub fn character_changes(&self) -> &[CharacterState] {
        &self.character_changes
    }

    pub fn world_change(&self) -> &StateChange<WorldState> {
        &self.world_change
    }

    pub fn memory_changes(&self) -> &[MemoryEntry] {
        &self.memory_changes
    }

    pub fn summary_delta(&self) -> Option<&str> {
        self.summary_delta.as_deref()
    }
}
