use crate::domain::asset::ids::NarrativeNodeKey;
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::CharacterId;
use crate::domain::knowledge::{CurrentPerception, KnowledgeEntry};
use crate::domain::narrative::{StoryEvent, StorySummary};
use crate::domain::narrative_graph::definition::NarrativeNodeState;
use crate::domain::story_instance::constraint::ActiveStoryConstraint;
use crate::domain::story_instance::snapshot::NarrativeConditionStateView;
use crate::domain::story_instance::state::{CharacterInstanceState, CurrentScene, RelationshipKey, RelationshipState};
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};
use crate::turn::turn_pipeline::TurnStage;
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

    pub fn to_turn_phase(self) -> crate::turn::turn_contract::TurnPhase {
        match self {
            ValidationDecision::Pass => crate::turn::turn_contract::TurnPhase::ReadyToCommit,
            ValidationDecision::Repair => crate::turn::turn_contract::TurnPhase::RepairRequired,
            ValidationDecision::Reject => crate::turn::turn_contract::TurnPhase::Failed,
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
pub struct CharacterInstanceStateChange {
    pub character_id: CharacterId,
    pub new_state: CharacterInstanceState,
}

#[derive(Debug, Clone)]
pub struct RelationshipStateChange {
    pub key: RelationshipKey,
    pub new_state: RelationshipState,
}

#[derive(Debug, Clone)]
pub struct ValidatedNarrativeChange {
    pub node_key: NarrativeNodeKey,
    pub from: NarrativeNodeState,
    pub to: NarrativeNodeState,
    pub expected_graph_revision: u64,
}

#[derive(Debug, Clone)]
pub struct ValidatedChangeSet {
    story_text: BoundedText,
    events: Vec<StoryEvent>,
    character_changes: Vec<CharacterInstanceStateChange>,
    relationship_changes: Vec<RelationshipStateChange>,
    knowledge_additions: Vec<KnowledgeEntry>,
    current_perceptions: Vec<CurrentPerception>,
    scene_change: StateChange<CurrentScene>,
    narrative_changes: Vec<ValidatedNarrativeChange>,
    condition_state: NarrativeConditionStateView,
    constraint_change: StateChange<Vec<ActiveStoryConstraint>>,
    summary_change: StateChange<StorySummary>,
}

impl ValidatedChangeSet {
    pub fn new(parts: ValidatedChangeSetParts) -> Result<Self, TurnExecutionError> {
        if parts.story_text.as_str().trim().is_empty() {
            return Err(TurnExecutionError::new(
                TurnFailureKind::ValidationRejected,
                "no_story_text",
                Some(TurnStage::Validation),
                "validated change set requires non-empty story text",
            ));
        }
        Ok(Self {
            story_text: parts.story_text,
            events: parts.events,
            character_changes: parts.character_changes,
            relationship_changes: parts.relationship_changes,
            knowledge_additions: parts.knowledge_additions,
            current_perceptions: parts.current_perceptions,
            scene_change: parts.scene_change,
            narrative_changes: parts.narrative_changes,
            condition_state: parts.condition_state,
            constraint_change: parts.constraint_change,
            summary_change: parts.summary_change,
        })
    }

    pub fn story_text(&self) -> &str {
        self.story_text.as_str()
    }

    pub fn events(&self) -> &[StoryEvent] {
        &self.events
    }

    pub fn character_changes(&self) -> &[CharacterInstanceStateChange] {
        &self.character_changes
    }

    pub fn relationship_changes(&self) -> &[RelationshipStateChange] {
        &self.relationship_changes
    }

    pub fn knowledge_additions(&self) -> &[KnowledgeEntry] {
        &self.knowledge_additions
    }

    pub fn current_perceptions(&self) -> &[CurrentPerception] {
        &self.current_perceptions
    }

    pub fn scene_change(&self) -> StateChange<CurrentScene> {
        self.scene_change.clone()
    }

    pub fn narrative_changes(&self) -> &[ValidatedNarrativeChange] {
        &self.narrative_changes
    }

    pub fn condition_state(&self) -> &NarrativeConditionStateView {
        &self.condition_state
    }

    pub fn constraint_change(&self) -> StateChange<Vec<ActiveStoryConstraint>> {
        self.constraint_change.clone()
    }

    pub fn summary_change(&self) -> StateChange<StorySummary> {
        self.summary_change.clone()
    }

    pub fn has_scene_change(&self) -> bool {
        matches!(self.scene_change, StateChange::Replace(_))
    }

    pub fn has_summary_change(&self) -> bool {
        matches!(self.summary_change, StateChange::Replace(_))
    }

    pub fn has_constraint_change(&self) -> bool {
        matches!(self.constraint_change, StateChange::Replace(_))
    }
}

pub struct ValidatedChangeSetParts {
    pub story_text: BoundedText,
    pub events: Vec<StoryEvent>,
    pub character_changes: Vec<CharacterInstanceStateChange>,
    pub relationship_changes: Vec<RelationshipStateChange>,
    pub knowledge_additions: Vec<KnowledgeEntry>,
    pub current_perceptions: Vec<CurrentPerception>,
    pub scene_change: StateChange<CurrentScene>,
    pub narrative_changes: Vec<ValidatedNarrativeChange>,
    pub condition_state: NarrativeConditionStateView,
    pub constraint_change: StateChange<Vec<ActiveStoryConstraint>>,
    pub summary_change: StateChange<StorySummary>,
}

fn invariant(code: &'static str, stage: Option<TurnStage>, message: impl Into<String>) -> TurnExecutionError {
    TurnExecutionError::new(TurnFailureKind::InvariantViolation, code, stage, message.into())
}

#[cfg(test)]
#[path = "tests/turn_validation_tests.rs"]
mod tests;
