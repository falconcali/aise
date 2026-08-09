use crate::core::story_proposal::{ProposedCharacterChange, ProposedWorldChange};
use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_error::TurnExecutionError;
use crate::core::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::core::turn_trace::{SpanPayload, ValidationData};
use crate::core::turn_validation::{
    BoundedValidationIssues, CharacterStateChange, MemoryStateChange, Repairability, StateChange, ValidatedChangeSet,
    ValidationIssue, ValidationResult,
};
use crate::domain::character::{CharacterState, InternalState, Relation};
use crate::domain::ids::{EventId, FactId, MemoryId};
use crate::domain::memory::MemoryEntry;
use crate::domain::narrative::StoryEvent;
use crate::domain::story_instance::snapshot::StoryReadSnapshot;
use crate::domain::world::{FactSource, WorldFact, WorldState};
use crate::validation::validators::DeterministicValidator;
use crate::validation::validators::consistency::ConsistencyValidator;
use crate::validation::validators::domain_invariant::DomainInvariantValidator;
use crate::validation::validators::knowledge_boundary::KnowledgeBoundaryValidator;
use crate::validation::validators::modification_permission::ModificationPermissionValidator;
use crate::validation::validators::player_control::PlayerControlValidator;
use crate::validation::validators::schema::SchemaValidator;
use crate::validation::validators::world_fact_evidence::WorldFactEvidenceValidator;
use async_trait::async_trait;

#[derive(Default)]
pub struct ValidationPipeline {
    schema: SchemaValidator,
    consistency: ConsistencyValidator,
    modification_permission: ModificationPermissionValidator,
    domain_invariant: DomainInvariantValidator,
    knowledge_boundary: KnowledgeBoundaryValidator,
    player_control: PlayerControlValidator,
    world_fact_evidence: WorldFactEvidenceValidator,
}

#[async_trait]
impl TurnExecutionPipeline for ValidationPipeline {
    fn stage(&self) -> TurnStage {
        TurnStage::Validation
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), TurnExecutionError> {
        let issues = self.run_deterministic(ctx)?;
        let max_issues = ctx.budget().max_validation_issues();
        let payload = SpanPayload::Validation(ValidationData {
            pass: issues.is_empty(),
            issues: issues
                .iter()
                .map(|issue| format!("{}: {}", issue.code, issue.message))
                .collect(),
        });
        let pending = ctx.trace().begin_span("aise.validation", "validation.execute");
        ctx.trace().end_span_with(pending, &payload);
        if issues.is_empty() {
            let change_set = build_change_set(ctx)?;
            let result = ValidationResult::pass(change_set);
            return ctx.set_validation_result(result);
        }
        let bounded = BoundedValidationIssues::try_new(issues, max_issues).map_err(|error| {
            TurnExecutionError::new(
                crate::core::turn_error::TurnFailureKind::InvariantViolation,
                "issue_limit_exceeded",
                Some(TurnStage::Validation),
                error.to_string(),
            )
        })?;
        let result = if bounded.issues().iter().any(|issue| issue.repairability == Repairability::Fatal) {
            ValidationResult::reject(bounded).map_err(|error| {
                TurnExecutionError::new(
                    crate::core::turn_error::TurnFailureKind::InvariantViolation,
                    "invalid_reject_result",
                    Some(TurnStage::Validation),
                    error.to_string(),
                )
            })?
        } else {
            ValidationResult::repair(bounded).map_err(|error| {
                TurnExecutionError::new(
                    crate::core::turn_error::TurnFailureKind::InvariantViolation,
                    "invalid_repair_result",
                    Some(TurnStage::Validation),
                    error.to_string(),
                )
            })?
        };
        ctx.set_validation_result(result)
    }
}

impl ValidationPipeline {
    fn run_deterministic(&self, ctx: &TurnExecutionContext) -> Result<Vec<ValidationIssue>, TurnExecutionError> {
        let mut issues = Vec::new();
        for validator in [
            &self.schema as &dyn DeterministicValidator,
            &self.consistency as &dyn DeterministicValidator,
            &self.modification_permission as &dyn DeterministicValidator,
            &self.domain_invariant as &dyn DeterministicValidator,
            &self.knowledge_boundary as &dyn DeterministicValidator,
            &self.player_control as &dyn DeterministicValidator,
            &self.world_fact_evidence as &dyn DeterministicValidator,
        ] {
            issues.extend(validator.validate(ctx)?);
        }
        Ok(issues)
    }
}

fn build_change_set(ctx: &TurnExecutionContext) -> Result<ValidatedChangeSet, TurnExecutionError> {
    let proposal = ctx
        .proposal()
        .ok_or_else(|| invariant("missing_proposal", "no story proposal produced"))?;
    let snapshot = ctx
        .snapshot()
        .ok_or_else(|| invariant("missing_snapshot", "no story snapshot available"))?;
    let turn_id = ctx.turn_id().clone();
    let events = proposal
        .events
        .iter()
        .enumerate()
        .map(|(seq, event)| StoryEvent {
            id: EventId::from(format!("{turn_id}#{seq}")),
            turn_id: turn_id.clone(),
            seq: seq as u32,
            kind: event.kind,
            payload: serde_json::json!({ "text": event.summary }),
        })
        .collect();
    let character_changes = apply_character_changes(snapshot, &proposal.character_changes)?;
    let world_change = apply_world_change(snapshot, &proposal.world_change)?;
    let known_character_ids: Vec<&crate::domain::ids::CharacterId> = snapshot.character_states().keys().collect();
    let memory_changes = proposal
        .memory_changes
        .iter()
        .enumerate()
        .filter(|(_, memory)| known_character_ids.contains(&&memory.owner))
        .map(|(seq, memory)| MemoryStateChange {
            character_id: memory.owner.clone(),
            entry: MemoryEntry {
                id: MemoryId::from(format!("{turn_id}#memory#{}#{}", memory.owner.as_str(), seq)),
                owner: memory.owner.clone(),
                kind: memory.kind,
                content: memory.content.clone(),
                created_at: ctx.identity().started_at_ms(),
            },
        })
        .collect();
    ValidatedChangeSet::new(
        proposal.story_text.clone(),
        events,
        character_changes,
        world_change,
        memory_changes,
        crate::core::turn_validation::StoryStateChanges {
            scene_change: proposal
                .scene_change
                .clone()
                .map_or(StateChange::Unchanged, StateChange::Replace),
            constraint_change: build_constraint_change(&proposal.constraint_changes, &turn_id)?,
            summary_change: proposal
                .summary_change
                .clone()
                .map_or(StateChange::Unchanged, StateChange::Replace),
        },
    )
}

fn build_constraint_change(
    texts: &[String],
    turn_id: &crate::domain::ids::TurnId,
) -> Result<StateChange<Vec<crate::domain::story_instance::constraint::ActiveStoryConstraint>>, TurnExecutionError> {
    if texts.is_empty() {
        return Ok(StateChange::Unchanged);
    }
    let mut constraints = Vec::with_capacity(texts.len());
    for (index, text) in texts.iter().enumerate() {
        let id = crate::domain::ids::ConstraintId::try_new(format!("{turn_id}#constraint#{index}"))
            .map_err(|_| invariant("invalid_constraint_id", "failed to build constraint id"))?;
        let statement =
            crate::domain::asset::validation::BoundedText::try_new(text.clone(), "constraint_statement", 512)
                .map_err(|_| invariant("invalid_constraint_text", "constraint statement exceeds bound"))?;
        constraints.push(crate::domain::story_instance::constraint::ActiveStoryConstraint {
            id,
            source: crate::domain::story_instance::constraint::StoryConstraintSource::CommittedTurn {
                turn_id: turn_id.clone(),
            },
            scope: crate::domain::asset::constraint::StoryConstraintScope::Story,
            requirement: crate::domain::asset::constraint::StoryConstraintRequirement::Require { statement },
            lifecycle: crate::domain::asset::constraint::StoryConstraintLifecycle::Persistent,
        });
    }
    Ok(StateChange::Replace(constraints))
}

fn apply_character_changes(
    snapshot: &StoryReadSnapshot,
    changes: &[ProposedCharacterChange],
) -> Result<Vec<CharacterStateChange>, TurnExecutionError> {
    let mut result = Vec::new();
    for change in changes {
        if !snapshot.character_states().contains_key(&change.character_id) {
            continue;
        }
        let mut state = CharacterState {
            id: change.character_id.clone(),
            name: change.character_id.as_str().to_owned(),
            bio: String::new(),
            internal_state: InternalState {
                goals: change.goal_updates.clone(),
                health: change.health_delta.unwrap_or(0),
                relationships: change
                    .affinity_deltas
                    .iter()
                    .map(|affinity| Relation {
                        other: affinity.other.clone(),
                        affinity: affinity.delta,
                    })
                    .collect(),
            },
        };
        if let Some(delta) = change.health_delta {
            state.internal_state.health = state.internal_state.health.saturating_add(delta);
        }
        result.push(CharacterStateChange {
            character_id: change.character_id.clone(),
            new_state: state,
        });
    }
    Ok(result)
}

fn apply_world_change(
    snapshot: &StoryReadSnapshot,
    change: &ProposedWorldChange,
) -> Result<StateChange<WorldState>, TurnExecutionError> {
    if change.add_facts.is_empty() {
        return Ok(StateChange::Unchanged);
    }
    let mut world = WorldState {
        id: snapshot.story_id().clone(),
        name: String::new(),
        facts: Vec::new(),
    };
    world
        .facts
        .extend(change.add_facts.iter().enumerate().map(|(offset, fact)| WorldFact {
            id: FactId::from(format!("{}-fact-{}", world.id.as_str(), offset + 1)),
            text: fact.text.clone(),
            source: FactSource::CommittedTurn,
        }));
    Ok(StateChange::Replace(world))
}

fn invariant(code: &'static str, message: impl Into<String>) -> TurnExecutionError {
    TurnExecutionError::new(
        crate::core::turn_error::TurnFailureKind::InvariantViolation,
        code,
        Some(TurnStage::Validation),
        message.into(),
    )
}
