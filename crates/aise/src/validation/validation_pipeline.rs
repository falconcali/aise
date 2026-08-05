use crate::core::story_proposal::{ProposedCharacterChange, ProposedWorldChange};
use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_data::StoryReadSnapshot;
use crate::core::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::core::turn_trace::{SpanPayload, ValidationData};
use crate::core::turn_validation::{
    StateChange, ValidatedChangeSet, ValidationDecision, ValidationIssue, ValidationResult, fatal,
};
use crate::domain::character::{CharacterState, Relation};
use crate::domain::ids::{EventId, FactId, MemoryId};
use crate::domain::memory::MemoryEntry;
use crate::domain::narrative::StoryEvent;
use crate::domain::world::{FactSource, WorldFact, WorldState};
use crate::error::AiseError;
use crate::validation::validators::consistency::ConsistencyValidator;
use crate::validation::validators::schema::SchemaValidator;
use async_trait::async_trait;

#[derive(Default)]
pub struct ValidationPipeline {
    schema: SchemaValidator,
    consistency: ConsistencyValidator,
}

#[async_trait]
impl TurnExecutionPipeline for ValidationPipeline {
    fn stage(&self) -> TurnStage {
        TurnStage::Validation
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), AiseError> {
        let mut result = {
            let pending = ctx.trace().begin_span("aise.validation", "schema.validate");
            let outcome = self.schema.validate(ctx);
            let payload = validation_payload(&outcome);
            ctx.trace().end_span_with(pending, &payload);
            outcome?
        };
        if result.is_pass() {
            let pending = ctx.trace().begin_span("aise.validation", "consistency.validate");
            let outcome = self.consistency.validate(ctx).await;
            let payload = validation_payload(&outcome);
            ctx.trace().end_span_with(pending, &payload);
            result = outcome?;
        }
        let change_set = match result.decision() {
            ValidationDecision::Pass => match build_change_set(ctx) {
                Ok(change_set) => Some(change_set),
                Err(issue) => {
                    result = ValidationResult::reject(&issue.code, &issue.message);
                    None
                }
            },
            _ => None,
        };
        ctx.set_validation_result(result, change_set)
    }
}

fn build_change_set(ctx: &TurnExecutionContext) -> Result<ValidatedChangeSet, ValidationIssue> {
    let proposal = ctx
        .proposal()
        .ok_or_else(|| fatal("missing_proposal", "no story proposal produced"))?;
    let snapshot = ctx
        .snapshot()
        .ok_or_else(|| fatal("missing_snapshot", "no story snapshot available"))?;
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
    let characters = apply_character_changes(snapshot, &proposal.character_changes)?;
    let world_change = apply_world_change(snapshot, &proposal.world_change)?;
    let memory_changes = proposal
        .memory_changes
        .iter()
        .map(|memory| MemoryEntry {
            id: MemoryId::from(format!("{turn_id}#memory#{}", memory.owner.as_str())),
            owner: memory.owner.clone(),
            kind: memory.kind,
            content: memory.content.clone(),
            created_at: ctx.identity().started_at_ms(),
        })
        .collect();
    Ok(ValidatedChangeSet::new(
        proposal.story_text.clone(),
        events,
        characters,
        world_change,
        memory_changes,
        proposal.summary_delta.clone(),
    ))
}

fn apply_character_changes(
    snapshot: &StoryReadSnapshot,
    changes: &[ProposedCharacterChange],
) -> Result<Vec<CharacterState>, ValidationIssue> {
    let mut characters = snapshot.characters().to_vec();
    for change in changes {
        let target = characters
            .iter_mut()
            .find(|character| character.id == change.character_id)
            .ok_or_else(|| {
                fatal(
                    "unknown_character",
                    format!("character change references unknown character {}", change.character_id.as_str()),
                )
            })?;
        if !change.goal_updates.is_empty() {
            target.internal_state.goals = change.goal_updates.clone();
        }
        if let Some(delta) = change.health_delta {
            target.internal_state.health = target.internal_state.health.saturating_add(delta);
        }
        for affinity in &change.affinity_deltas {
            match target
                .internal_state
                .relationships
                .iter_mut()
                .find(|relation| relation.other == affinity.other)
            {
                Some(relation) => relation.affinity = relation.affinity.saturating_add(affinity.delta),
                None => target.internal_state.relationships.push(Relation {
                    other: affinity.other.clone(),
                    affinity: affinity.delta,
                }),
            }
        }
    }
    Ok(characters)
}

fn apply_world_change(
    snapshot: &StoryReadSnapshot,
    change: &ProposedWorldChange,
) -> Result<StateChange<WorldState>, ValidationIssue> {
    if change.add_facts.is_empty() {
        return Ok(StateChange::Unchanged);
    }
    let mut world = snapshot
        .world()
        .cloned()
        .ok_or_else(|| fatal("missing_world", "world change requires an existing world state to extend"))?;
    let next_seq = world.facts.len();
    world
        .facts
        .extend(change.add_facts.iter().enumerate().map(|(offset, text)| WorldFact {
            id: FactId::from(format!("{}-fact-{}", world.id.as_str(), next_seq + offset)),
            text: text.clone(),
            source: FactSource::CommittedTurn,
        }));
    Ok(StateChange::Replace(world))
}

fn validation_payload(outcome: &Result<ValidationResult, AiseError>) -> SpanPayload {
    match outcome {
        Ok(result) => SpanPayload::Validation(ValidationData {
            pass: result.is_pass(),
            issues: result
                .issues()
                .iter()
                .map(|issue| format!("{}: {}", issue.code, issue.message))
                .collect(),
        }),
        Err(error) => SpanPayload::Validation(ValidationData {
            pass: false,
            issues: vec![error.to_string()],
        }),
    }
}
