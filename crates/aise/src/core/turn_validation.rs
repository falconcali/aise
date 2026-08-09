use crate::core::turn_error::{TurnExecutionError, TurnFailureKind};
use crate::core::turn_pipeline::TurnStage;
use crate::domain::character::CharacterState;
use crate::domain::ids::CharacterId;
use crate::domain::memory::MemoryEntry;
use crate::domain::narrative::{StoryEvent, StorySummary};
use crate::domain::story_instance::constraint::ActiveStoryConstraint;
use crate::domain::story_instance::state::CurrentScene;
use crate::domain::world::WorldState;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationIssueCode {
    SchemaInvalid,
    ReferenceMissing,
    ModificationForbidden,
    DomainInvariantViolated,
    KnowledgeBoundaryViolated,
    PlayerControlViolated,
    WorldFactEvidenceMissing,
    WorldFactEvidenceInvalid,
    NarrativeInconsistent,
    CharacterInconsistent,
}

impl ValidationIssueCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ValidationIssueCode::SchemaInvalid => "schema_invalid",
            ValidationIssueCode::ReferenceMissing => "reference_missing",
            ValidationIssueCode::ModificationForbidden => "modification_forbidden",
            ValidationIssueCode::DomainInvariantViolated => "domain_invariant_violated",
            ValidationIssueCode::KnowledgeBoundaryViolated => "knowledge_boundary_violated",
            ValidationIssueCode::PlayerControlViolated => "player_control_violated",
            ValidationIssueCode::WorldFactEvidenceMissing => "world_fact_evidence_missing",
            ValidationIssueCode::WorldFactEvidenceInvalid => "world_fact_evidence_invalid",
            ValidationIssueCode::NarrativeInconsistent => "narrative_inconsistent",
            ValidationIssueCode::CharacterInconsistent => "character_inconsistent",
        }
    }
}

impl std::fmt::Display for ValidationIssueCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Repairability {
    Repairable,
    Fatal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationLocation {
    pub path: String,
    pub item_index: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub code: ValidationIssueCode,
    pub message: String,
    pub repairability: Repairability,
    pub location: Option<ValidationLocation>,
}

#[derive(Debug, Clone)]
pub struct BoundedValidationIssues {
    issues: Vec<ValidationIssue>,
    max_issues: usize,
}

impl BoundedValidationIssues {
    pub fn try_new(issues: Vec<ValidationIssue>, max_issues: usize) -> Result<Self, TurnExecutionError> {
        if max_issues == 0 {
            return Err(invariant(
                "zero_issue_limit",
                Some(TurnStage::Validation),
                "max_issues must be positive",
            ));
        }
        if issues.len() > max_issues {
            return Err(invariant(
                "issue_limit_exceeded",
                Some(TurnStage::Validation),
                format!("validation issues {} exceeds limit {max_issues}", issues.len()),
            ));
        }
        for issue in &issues {
            if issue.message.chars().count() > 500 {
                return Err(invariant(
                    "issue_message_too_long",
                    Some(TurnStage::Validation),
                    "validation issue message exceeds 500 chars",
                ));
            }
        }
        Ok(Self { issues, max_issues })
    }

    pub fn issues(&self) -> &[ValidationIssue] {
        &self.issues
    }

    pub fn is_empty(&self) -> bool {
        self.issues.is_empty()
    }

    pub fn into_issues(self) -> Vec<ValidationIssue> {
        self.issues
    }

    pub fn max_issues(&self) -> usize {
        self.max_issues
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationDecision {
    Pass,
    Repair,
    Reject,
}

impl ValidationDecision {
    pub fn as_str(&self) -> &'static str {
        match self {
            ValidationDecision::Pass => "pass",
            ValidationDecision::Repair => "repair",
            ValidationDecision::Reject => "reject",
        }
    }

    pub fn to_turn_phase(self) -> crate::core::turn_contract::TurnPhase {
        match self {
            ValidationDecision::Pass => crate::core::turn_contract::TurnPhase::ReadyToCommit,
            ValidationDecision::Repair => crate::core::turn_contract::TurnPhase::RepairRequired,
            ValidationDecision::Reject => crate::core::turn_contract::TurnPhase::Failed,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ValidationResult {
    Pass(Box<ValidatedChangeSet>),
    Repair(BoundedValidationIssues),
    Reject(BoundedValidationIssues),
}

impl ValidationResult {
    pub(crate) fn pass(change_set: ValidatedChangeSet) -> Self {
        Self::Pass(Box::new(change_set))
    }

    pub fn repair(issues: BoundedValidationIssues) -> Result<Self, TurnExecutionError> {
        if issues.is_empty() {
            return Err(invariant(
                "empty_repair_issues",
                Some(TurnStage::Validation),
                "Repair requires at least one issue",
            ));
        }
        for issue in issues.issues() {
            if issue.repairability == Repairability::Fatal {
                return Err(invariant(
                    "fatal_issue_in_repair",
                    Some(TurnStage::Validation),
                    "Repair cannot contain a fatal issue",
                ));
            }
        }
        Ok(Self::Repair(issues))
    }

    pub fn reject(issues: BoundedValidationIssues) -> Result<Self, TurnExecutionError> {
        if !issues.issues().iter().any(|issue| issue.repairability == Repairability::Fatal) {
            return Err(invariant(
                "reject_requires_fatal_issue",
                Some(TurnStage::Validation),
                "Reject requires at least one fatal issue",
            ));
        }
        Ok(Self::Reject(issues))
    }

    pub fn decision(&self) -> ValidationDecision {
        match self {
            ValidationResult::Pass(_) => ValidationDecision::Pass,
            ValidationResult::Repair(_) => ValidationDecision::Repair,
            ValidationResult::Reject(_) => ValidationDecision::Reject,
        }
    }

    pub fn issues(&self) -> &[ValidationIssue] {
        match self {
            ValidationResult::Pass(_) => &[],
            ValidationResult::Repair(issues) | ValidationResult::Reject(issues) => issues.issues(),
        }
    }

    pub(crate) fn into_change_set(self) -> Option<ValidatedChangeSet> {
        match self {
            ValidationResult::Pass(change_set) => Some(*change_set),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateChange<T> {
    Unchanged,
    Replace(T),
}

impl<T> StateChange<T> {
    pub fn as_ref(&self) -> Option<&T> {
        match self {
            StateChange::Unchanged => None,
            StateChange::Replace(value) => Some(value),
        }
    }

    pub fn into_value(self) -> Option<T> {
        match self {
            StateChange::Unchanged => None,
            StateChange::Replace(value) => Some(value),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CharacterStateChange {
    pub character_id: CharacterId,
    pub new_state: CharacterState,
}

#[derive(Debug, Clone)]
pub struct MemoryStateChange {
    pub character_id: CharacterId,
    pub entry: MemoryEntry,
}

#[derive(Debug, Clone)]
pub struct StoryStateChanges {
    pub scene_change: StateChange<CurrentScene>,
    pub constraint_change: StateChange<Vec<ActiveStoryConstraint>>,
    pub summary_change: StateChange<StorySummary>,
}

#[derive(Debug, Clone)]
pub struct ValidatedChangeSet {
    story_text: String,
    events: Vec<StoryEvent>,
    character_changes: Vec<CharacterStateChange>,
    world_change: StateChange<WorldState>,
    memory_changes: Vec<MemoryStateChange>,
    story_state: StoryStateChanges,
}

impl ValidatedChangeSet {
    pub(crate) fn new(
        story_text: String,
        events: Vec<StoryEvent>,
        character_changes: Vec<CharacterStateChange>,
        world_change: StateChange<WorldState>,
        memory_changes: Vec<MemoryStateChange>,
        story_state: StoryStateChanges,
    ) -> Result<Self, TurnExecutionError> {
        if story_text.trim().is_empty() {
            return Err(TurnExecutionError::new(
                TurnFailureKind::ValidationRejected,
                "no_story_text",
                Some(TurnStage::Validation),
                "validated change set requires non-empty story text",
            ));
        }
        Ok(Self {
            story_text,
            events,
            character_changes,
            world_change,
            memory_changes,
            story_state,
        })
    }

    pub fn story_text(&self) -> &str {
        &self.story_text
    }

    pub fn events(&self) -> &[StoryEvent] {
        &self.events
    }

    pub fn character_changes(&self) -> &[CharacterStateChange] {
        &self.character_changes
    }

    pub fn world_change(&self) -> StateChange<WorldState> {
        match &self.world_change {
            StateChange::Unchanged => StateChange::Unchanged,
            StateChange::Replace(world) => StateChange::Replace(world.clone()),
        }
    }

    pub fn world_change_ref(&self) -> Option<&WorldState> {
        self.world_change.as_ref()
    }

    pub fn memory_changes(&self) -> &[MemoryStateChange] {
        &self.memory_changes
    }

    pub fn scene_change(&self) -> StateChange<CurrentScene> {
        self.story_state.scene_change.clone()
    }

    pub fn constraint_change(&self) -> StateChange<Vec<ActiveStoryConstraint>> {
        self.story_state.constraint_change.clone()
    }

    pub fn summary_change(&self) -> StateChange<StorySummary> {
        self.story_state.summary_change.clone()
    }

    pub fn has_world_change(&self) -> bool {
        matches!(self.world_change, StateChange::Replace(_))
    }

    pub fn has_scene_change(&self) -> bool {
        matches!(self.story_state.scene_change, StateChange::Replace(_))
    }

    pub fn has_summary_change(&self) -> bool {
        matches!(self.story_state.summary_change, StateChange::Replace(_))
    }

    pub fn has_constraint_change(&self) -> bool {
        matches!(self.story_state.constraint_change, StateChange::Replace(_))
    }
}

fn invariant(code: &'static str, stage: Option<TurnStage>, message: impl Into<String>) -> TurnExecutionError {
    TurnExecutionError::new(TurnFailureKind::InvariantViolation, code, stage, message.into())
}

#[cfg(test)]
#[path = "tests/turn_validation_tests.rs"]
mod tests;
