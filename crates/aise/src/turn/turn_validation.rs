use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::RoleId;
use crate::domain::knowledge::{KnowledgeEntry, KnowledgeSourceId};
use crate::domain::narrative::StoryEvent;
use crate::domain::story_instance::constraint::ActiveStoryConstraint;
use crate::domain::story_instance::role::StoryRoleState;
use crate::domain::story_instance::state::{RelationshipKey, RelationshipState};
use crate::domain::turn::{DeletableKnowledgeId, ValidatedNarrativeResolution};
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};
use crate::turn::turn_pipeline::TurnStage;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationIssueCode {
    StoryTextEmpty,
    StoryTextExceedsBounds,
    ExtractionSchemaInvalid,
    ExtractionCountExceeded,
    ExtractionDuplicateTarget,
    ReferenceMissing,
    ModificationForbidden,
    UnchangedRoleEmitted,
    UnchangedRelationshipEmitted,
    DomainInvariantViolated,
    KnowledgeOperationIllegal,
    KnowledgeTargetInvalid,
    KnowledgeOwnerUnauthorized,
    StaleStateExtraction,
    StoryStateInconsistent,
    NarrativeInconsistent,
}

impl ValidationIssueCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ValidationIssueCode::StoryTextEmpty => "story_text_empty",
            ValidationIssueCode::StoryTextExceedsBounds => "story_text_exceeds_bounds",
            ValidationIssueCode::ExtractionSchemaInvalid => "extraction_schema_invalid",
            ValidationIssueCode::ExtractionCountExceeded => "extraction_count_exceeded",
            ValidationIssueCode::ExtractionDuplicateTarget => "extraction_duplicate_target",
            ValidationIssueCode::ReferenceMissing => "reference_missing",
            ValidationIssueCode::ModificationForbidden => "modification_forbidden",
            ValidationIssueCode::UnchangedRoleEmitted => "unchanged_role_emitted",
            ValidationIssueCode::UnchangedRelationshipEmitted => "unchanged_relationship_emitted",
            ValidationIssueCode::DomainInvariantViolated => "domain_invariant_violated",
            ValidationIssueCode::KnowledgeOperationIllegal => "knowledge_operation_illegal",
            ValidationIssueCode::KnowledgeTargetInvalid => "knowledge_target_invalid",
            ValidationIssueCode::KnowledgeOwnerUnauthorized => "knowledge_owner_unauthorized",
            ValidationIssueCode::StaleStateExtraction => "stale_state_extraction",
            ValidationIssueCode::StoryStateInconsistent => "story_state_inconsistent",
            ValidationIssueCode::NarrativeInconsistent => "narrative_inconsistent",
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
pub enum ValidationIssueClass {
    Story,
    Extraction,
    CrossConsistency,
}

impl ValidationIssueClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            ValidationIssueClass::Story => "story",
            ValidationIssueClass::Extraction => "extraction",
            ValidationIssueClass::CrossConsistency => "cross_consistency",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationRemedy {
    RepairStory,
    ReextractState,
    Reject,
}

impl ValidationRemedy {
    pub fn as_str(&self) -> &'static str {
        match self {
            ValidationRemedy::RepairStory => "repair_story",
            ValidationRemedy::ReextractState => "reextract_state",
            ValidationRemedy::Reject => "reject",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationLocation {
    pub path: String,
    pub item_index: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub code: ValidationIssueCode,
    pub class: ValidationIssueClass,
    pub remedy: ValidationRemedy,
    pub message: String,
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
        if issues.is_empty() {
            return Err(invariant(
                "empty_issue_set",
                Some(TurnStage::Validation),
                "a non-pass validation result requires at least one issue",
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

    pub fn sorted_codes(&self) -> Vec<ValidationIssueCode> {
        let mut codes = self.issues.iter().map(|issue| issue.code).collect::<Vec<_>>();
        codes.sort_by_key(|code| code.as_str());
        codes
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationDecision {
    Pass,
    RepairStory,
    ReextractState,
    Reject,
}

impl ValidationDecision {
    pub fn as_str(&self) -> &'static str {
        match self {
            ValidationDecision::Pass => "pass",
            ValidationDecision::RepairStory => "repair_story",
            ValidationDecision::ReextractState => "reextract_state",
            ValidationDecision::Reject => "reject",
        }
    }

    pub fn to_turn_phase(self) -> crate::turn::turn_contract::TurnPhase {
        match self {
            ValidationDecision::Pass => crate::turn::turn_contract::TurnPhase::ReadyToCommit,
            ValidationDecision::RepairStory => crate::turn::turn_contract::TurnPhase::StoryRepairRequired,
            ValidationDecision::ReextractState => crate::turn::turn_contract::TurnPhase::StateReextractionRequired,
            ValidationDecision::Reject => crate::turn::turn_contract::TurnPhase::Failed,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ValidationResult {
    Pass(Box<ValidatedChangeSet>),
    RepairStory(BoundedValidationIssues),
    ReextractState(BoundedValidationIssues),
    Reject(BoundedValidationIssues),
}

impl ValidationResult {
    pub(crate) fn pass(change_set: ValidatedChangeSet) -> Self {
        Self::Pass(Box::new(change_set))
    }

    pub fn from_issues(issues: Vec<ValidationIssue>, max_issues: usize) -> Result<Self, TurnExecutionError> {
        if issues.is_empty() {
            return Err(invariant(
                "pass_requires_issue_construction",
                Some(TurnStage::Validation),
                "use ValidationResult::pass for the empty-issue case",
            ));
        }
        if issues.iter().any(|issue| issue.remedy == ValidationRemedy::Reject) {
            let bounded = BoundedValidationIssues::try_new(issues, max_issues)?;
            return Ok(Self::Reject(bounded));
        }
        if issues.iter().any(|issue| issue.remedy == ValidationRemedy::RepairStory) {
            let bounded = BoundedValidationIssues::try_new(issues, max_issues)?;
            return Ok(Self::RepairStory(bounded));
        }
        if issues.iter().all(|issue| issue.remedy == ValidationRemedy::ReextractState) {
            let bounded = BoundedValidationIssues::try_new(issues, max_issues)?;
            return Ok(Self::ReextractState(bounded));
        }
        Err(invariant(
            "inconsistent_remedy_set",
            Some(TurnStage::Validation),
            "validation issue remedies are inconsistent with decision reduction",
        ))
    }

    pub fn decision(&self) -> ValidationDecision {
        match self {
            ValidationResult::Pass(_) => ValidationDecision::Pass,
            ValidationResult::RepairStory(_) => ValidationDecision::RepairStory,
            ValidationResult::ReextractState(_) => ValidationDecision::ReextractState,
            ValidationResult::Reject(_) => ValidationDecision::Reject,
        }
    }

    pub fn issues(&self) -> &[ValidationIssue] {
        match self {
            ValidationResult::Pass(_) => &[],
            ValidationResult::RepairStory(issues)
            | ValidationResult::ReextractState(issues)
            | ValidationResult::Reject(issues) => issues.issues(),
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
pub struct RoleStateChange {
    pub role_id: RoleId,
    pub new_state: StoryRoleState,
}

#[derive(Debug, Clone)]
pub struct RelationshipStateChange {
    pub key: RelationshipKey,
    pub new_state: RelationshipState,
}

#[derive(Debug, Clone)]
pub enum ValidatedKnowledgeOperation {
    Add(KnowledgeEntry),
    Update {
        target: KnowledgeSourceId,
        value: KnowledgeEntry,
    },
    Delete {
        target: DeletableKnowledgeId,
    },
}

#[derive(Debug, Clone)]
pub struct ValidatedKnowledgeMutation {
    pub ordinal: u32,
    pub operation: ValidatedKnowledgeOperation,
}

#[derive(Debug, Clone)]
pub struct ValidatedChangeSet {
    story_text: BoundedText,
    role_changes: Vec<RoleStateChange>,
    relationship_changes: Vec<RelationshipStateChange>,
    knowledge_mutations: Vec<ValidatedKnowledgeMutation>,
    narrative_events: Vec<StoryEvent>,
    narrative_resolution: ValidatedNarrativeResolution,
    constraint_change: StateChange<Vec<ActiveStoryConstraint>>,
}

impl ValidatedChangeSet {
    pub fn new(parts: ValidatedChangeSetParts) -> Result<Self, TurnExecutionError> {
        if parts.story_text.as_str().trim().is_empty() {
            return Err(TurnExecutionError::new(
                TurnFailureKind::ValidationRejected,
                "story_text_empty",
                Some(TurnStage::Validation),
                "validated change set requires non-empty story text",
            ));
        }
        Ok(Self {
            story_text: parts.story_text,
            role_changes: parts.role_changes,
            relationship_changes: parts.relationship_changes,
            knowledge_mutations: parts.knowledge_mutations,
            narrative_events: parts.narrative_events,
            narrative_resolution: parts.narrative_resolution,
            constraint_change: parts.constraint_change,
        })
    }

    pub fn story_text(&self) -> &str {
        self.story_text.as_str()
    }

    pub fn role_changes(&self) -> &[RoleStateChange] {
        &self.role_changes
    }

    pub fn relationship_changes(&self) -> &[RelationshipStateChange] {
        &self.relationship_changes
    }

    pub fn knowledge_mutations(&self) -> &[ValidatedKnowledgeMutation] {
        &self.knowledge_mutations
    }

    pub fn narrative_events(&self) -> &[StoryEvent] {
        &self.narrative_events
    }

    pub fn narrative_resolution(&self) -> &ValidatedNarrativeResolution {
        &self.narrative_resolution
    }

    pub fn constraint_change(&self) -> StateChange<Vec<ActiveStoryConstraint>> {
        self.constraint_change.clone()
    }

    pub fn has_constraint_change(&self) -> bool {
        matches!(self.constraint_change, StateChange::Replace(_))
    }
}

pub struct ValidatedChangeSetParts {
    pub story_text: BoundedText,
    pub role_changes: Vec<RoleStateChange>,
    pub relationship_changes: Vec<RelationshipStateChange>,
    pub knowledge_mutations: Vec<ValidatedKnowledgeMutation>,
    pub narrative_events: Vec<StoryEvent>,
    pub narrative_resolution: ValidatedNarrativeResolution,
    pub constraint_change: StateChange<Vec<ActiveStoryConstraint>>,
}

fn invariant(code: &'static str, stage: Option<TurnStage>, message: impl Into<String>) -> TurnExecutionError {
    TurnExecutionError::new(TurnFailureKind::InvariantViolation, code, stage, message.into())
}

#[cfg(test)]
#[path = "tests/turn_validation_tests.rs"]
mod tests;
