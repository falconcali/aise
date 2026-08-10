use crate::core::story_proposal::{ProposedKnowledgeChange, WorldFactEvidenceRef};
use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_error::TurnExecutionError;
use crate::core::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::core::turn_trace::{SpanPayload, ValidationData};
use crate::core::turn_validation::{
    BoundedValidationIssues, CharacterInstanceStateChange, RelationshipStateChange, Repairability, StateChange,
    ValidatedChangeSet, ValidatedChangeSetParts, ValidatedNarrativeChange, ValidationIssue, ValidationResult,
};
use crate::domain::asset::constraint::StoryConstraintLifecycle;
use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::{EventId, FactId, MemoryId, StoryRevision};
use crate::domain::knowledge::KnowledgeEntry;
use crate::domain::knowledge::fact::WorldFact;
use crate::domain::knowledge::memory::MemoryEntry;
use crate::domain::knowledge::query::{CurrentPerception, KnowledgeSource};
use crate::domain::knowledge::rumor::SharedRumor;
use crate::domain::narrative::{StoryEvent, StorySummary};
use crate::domain::narrative_graph::definition::NarrativeNodeState;
use crate::validation::validators::DeterministicValidator;
use crate::validation::validators::consistency::ConsistencyValidator;
use crate::validation::validators::domain_invariant::DomainInvariantValidator;
use crate::validation::validators::knowledge_boundary::KnowledgeBoundaryValidator;
use crate::validation::validators::player_control::PlayerControlValidator;
use crate::validation::validators::schema::SchemaValidator;
use crate::validation::validators::world_fact_evidence::WorldFactEvidenceValidator;
use async_trait::async_trait;
use std::collections::BTreeMap;

#[derive(Default)]
pub struct ValidationPipeline {
    schema: SchemaValidator,
    consistency: ConsistencyValidator,
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
            return ctx.set_validation_result(ValidationResult::pass(build_change_set(ctx)?));
        }
        let bounded = BoundedValidationIssues::try_new(issues, ctx.budget().max_validation_issues())?;
        let result = if bounded.issues().iter().any(|issue| issue.repairability == Repairability::Fatal) {
            ValidationResult::reject(bounded)?
        } else {
            ValidationResult::repair(bounded)?
        };
        ctx.set_validation_result(result)
    }
}

impl ValidationPipeline {
    fn run_deterministic(&self, ctx: &TurnExecutionContext) -> Result<Vec<ValidationIssue>, TurnExecutionError> {
        let mut issues = Vec::new();
        for validator in [
            &self.schema as &dyn DeterministicValidator,
            &self.consistency,
            &self.domain_invariant,
            &self.knowledge_boundary,
            &self.player_control,
            &self.world_fact_evidence,
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
    let plan = ctx
        .plan()
        .ok_or_else(|| invariant("missing_writer_plan", "no writer plan available"))?;
    let turn_id = ctx.turn_id().clone();
    let committed_revision = StoryRevision::new(
        snapshot
            .base_revision()
            .get()
            .checked_add(1)
            .ok_or_else(|| invariant("story_revision_overflow", "story revision overflow"))?,
    );
    let story_text = bounded(&proposal.story_text, "story_text", ctx.budget().max_proposal_bytes())?;
    let mut events = proposal
        .events
        .iter()
        .enumerate()
        .map(|(index, event)| make_event(&turn_id, index, event.kind, serde_json::json!({"summary": event.summary})))
        .collect::<Result<Vec<_>, _>>()?;
    for intent in &plan.narrative_plan.global_event_intents {
        let index = events.len();
        events.push(make_event(
            &turn_id,
            index,
            crate::domain::narrative::EventKind::WorldChange,
            serde_json::json!({
                "event_key": intent.event_key,
                "category": intent.category,
                "participants": intent.participants,
                "location": intent.location,
                "description": intent.description,
            }),
        )?);
    }
    let character_changes = proposal
        .character_changes
        .iter()
        .map(|change| {
            let mut state = snapshot.character_states()[&change.character_id].clone();
            if let Some(location) = &change.location {
                state.location = location.clone();
            }
            if let Some(goals) = &change.goals {
                state.goals = goals
                    .iter()
                    .map(|goal| bounded(goal, "character_goal", ctx.budget().max_item_bytes()))
                    .collect::<Result<Vec<_>, _>>()?;
            }
            for (key, value) in &change.attribute_updates {
                state.attributes.insert(key.clone(), value.clone());
            }
            Ok(CharacterInstanceStateChange {
                character_id: change.character_id.clone(),
                new_state: state,
            })
        })
        .collect::<Result<Vec<_>, TurnExecutionError>>()?;
    let relationships = snapshot
        .relationships()
        .iter()
        .map(|relationship| (relationship.key(), relationship.clone()))
        .collect::<BTreeMap<_, _>>();
    let relationship_changes = proposal
        .relationship_changes
        .iter()
        .map(|change| {
            let key = crate::domain::story_instance::state::RelationshipKey {
                source_character_id: change.source_character_id.clone(),
                target_character_id: change.target_character_id.clone(),
                kind: change.kind.clone(),
            };
            let mut state = relationships[&key].clone();
            state.trust = state
                .trust
                .checked_add(change.trust_delta)
                .ok_or_else(|| invariant("relationship_trust_overflow", "relationship trust overflow"))?;
            Ok(RelationshipStateChange { key, new_state: state })
        })
        .collect::<Result<Vec<_>, TurnExecutionError>>()?;
    let knowledge_additions = proposal
        .knowledge_changes
        .iter()
        .enumerate()
        .map(|(index, change)| {
            make_knowledge_entry(
                change,
                index,
                &turn_id,
                committed_revision,
                &events,
                ctx.identity().started_at_ms(),
                ctx.budget().max_item_bytes(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let current_perceptions = proposal
        .perceptions
        .iter()
        .map(|perception| {
            let event =
                events
                    .get(usize::try_from(perception.source_event_index).map_err(|_| {
                        invariant("perception_event_index_invalid", "perception event index is invalid")
                    })?)
                    .ok_or_else(|| invariant("perception_event_missing", "perception event is missing"))?;
            Ok(CurrentPerception {
                character_id: perception.character_id.clone(),
                source_event_id: event.id.clone(),
                content: bounded(&perception.content, "perception", ctx.budget().max_item_bytes())?,
                story_revision: committed_revision,
            })
        })
        .collect::<Result<Vec<_>, TurnExecutionError>>()?;
    let narrative_changes = plan
        .narrative_plan
        .proposed_transitions
        .iter()
        .map(|change| ValidatedNarrativeChange {
            node_key: change.node_key.clone(),
            from: change.from,
            to: change.to,
            expected_graph_revision: change.expected_graph_revision,
        })
        .collect::<Vec<_>>();
    let mut condition_state = snapshot.condition_state().clone();
    for intent in &plan.narrative_plan.global_event_intents {
        condition_state.occurred_event_keys.insert(intent.event_key.clone());
    }
    let committed_sequence = snapshot
        .story_continuity()
        .next_sequence()
        .map_err(|_| invariant("story_sequence_overflow", "failed to assign the committed story sequence"))?;
    let mut final_nodes = snapshot.narrative_state().node_states.clone();
    for change in &narrative_changes {
        final_nodes.insert(change.node_key.clone(), change.to);
    }
    let constraints = snapshot
        .active_constraints()
        .iter()
        .filter(|constraint| match &constraint.lifecycle {
            StoryConstraintLifecycle::Persistent => true,
            StoryConstraintLifecycle::ThroughSequence { sequence } => sequence.get() > committed_sequence.get(),
            StoryConstraintLifecycle::UntilNarrativeNodeResolved { node_key } => !matches!(
                final_nodes.get(node_key),
                Some(NarrativeNodeState::Completed | NarrativeNodeState::Skipped)
            ),
        })
        .cloned()
        .collect::<Vec<_>>();
    let constraint_change = if constraints == snapshot.active_constraints() {
        StateChange::Unchanged
    } else {
        StateChange::Replace(constraints)
    };
    let summary_change = match proposal.summary_text.as_deref().map(str::trim) {
        None | Some("") => StateChange::Unchanged,
        Some(text) => {
            let boundary = snapshot
                .story_continuity()
                .latest_sequence()
                .ok_or_else(|| invariant("summary_boundary_missing", "summary requires a pre-turn sequence"))?;
            StateChange::Replace(StorySummary {
                text: bounded(text, "summary", ctx.budget().max_proposal_bytes())?,
                summarized_through: Some(boundary),
            })
        }
    };
    ValidatedChangeSet::new(ValidatedChangeSetParts {
        story_text,
        events,
        character_changes,
        relationship_changes,
        knowledge_additions,
        current_perceptions,
        scene_change: proposal
            .scene_change
            .clone()
            .map_or(StateChange::Unchanged, StateChange::Replace),
        narrative_changes,
        condition_state,
        constraint_change,
        summary_change,
    })
}

fn make_event(
    turn_id: &crate::domain::ids::TurnId,
    index: usize,
    kind: crate::domain::narrative::EventKind,
    payload: serde_json::Value,
) -> Result<StoryEvent, TurnExecutionError> {
    Ok(StoryEvent {
        id: EventId::from(format!("{}:event:{index}", turn_id.as_str())),
        turn_id: turn_id.clone(),
        seq: u32::try_from(index).map_err(|_| invariant("event_count_overflow", "event count exceeds u32"))?,
        kind,
        payload,
    })
}

fn make_knowledge_entry(
    change: &ProposedKnowledgeChange,
    index: usize,
    turn_id: &crate::domain::ids::TurnId,
    revision: StoryRevision,
    events: &[StoryEvent],
    created_at_ms: i64,
    max_bytes: usize,
) -> Result<KnowledgeEntry, TurnExecutionError> {
    match change {
        ProposedKnowledgeChange::Fact {
            content,
            proposition,
            entities,
            topics,
            salience,
            evidence,
        } => {
            let source_event = evidence.iter().find_map(|evidence| match evidence {
                WorldFactEvidenceRef::ProposedEvent { event_index } => {
                    usize::try_from(*event_index).ok().and_then(|index| events.get(index))
                }
                WorldFactEvidenceRef::SnapshotFact(_) => None,
            });
            Ok(KnowledgeEntry::Fact(WorldFact {
                id: FactId::from(format!("{}:fact:{index}", turn_id.as_str())),
                key: None,
                text: bounded(content, "fact_content", max_bytes)?,
                proposition: proposition.clone(),
                entities: canonical(entities.clone()),
                topics: canonical(topics.clone()),
                salience: *salience,
                source: KnowledgeSource::CommittedTurn {
                    turn_id: turn_id.clone(),
                    event_id: source_event.map(|event| event.id.clone()),
                },
                story_revision: revision,
            }))
        }
        ProposedKnowledgeChange::Rumor {
            content,
            claim,
            entities,
            topics,
            salience,
            source_character_id,
            truth_value,
            source_event_index,
        } => Ok(KnowledgeEntry::Rumor(SharedRumor {
            id: crate::domain::ids::RumorId::from(format!("{}:rumor:{index}", turn_id.as_str())),
            key: None,
            content: bounded(content, "rumor_content", max_bytes)?,
            claim: claim.clone(),
            entities: canonical(entities.clone()),
            topics: canonical(topics.clone()),
            salience: *salience,
            source_role_key: None,
            source_character_id: source_character_id.clone(),
            truth_value: truth_value.clone(),
            source: KnowledgeSource::CommittedTurn {
                turn_id: turn_id.clone(),
                event_id: event_id(events, *source_event_index)?,
            },
            story_revision: revision,
        })),
        ProposedKnowledgeChange::Memory {
            owner,
            memory_kind,
            content,
            entities,
            topics,
            salience,
            source_event_index,
        } => {
            let mut entities = entities.clone();
            entities.push(KnowledgeEntity::Character(owner.clone()));
            Ok(KnowledgeEntry::Memory(MemoryEntry {
                id: MemoryId::from(format!("{}:memory:{}:{index}", turn_id.as_str(), owner.as_str())),
                owner: owner.clone(),
                kind: memory_kind.clone(),
                content: bounded(content, "memory_content", max_bytes)?,
                entities: canonical(entities),
                topics: canonical(topics.clone()),
                salience: *salience,
                source: KnowledgeSource::CommittedTurn {
                    turn_id: turn_id.clone(),
                    event_id: event_id(events, *source_event_index)?,
                },
                story_revision: revision,
                created_at_ms,
            }))
        }
    }
}

fn event_id(events: &[StoryEvent], index: Option<u32>) -> Result<Option<EventId>, TurnExecutionError> {
    index
        .map(|index| {
            events
                .get(
                    usize::try_from(index)
                        .map_err(|_| invariant("knowledge_event_index_invalid", "knowledge event index is invalid"))?,
                )
                .map(|event| event.id.clone())
                .ok_or_else(|| invariant("knowledge_event_missing", "knowledge event is missing"))
        })
        .transpose()
}

fn canonical<T: Ord>(mut values: Vec<T>) -> Vec<T> {
    values.sort();
    values.dedup();
    values
}

fn bounded(value: &str, field: &'static str, maximum: usize) -> Result<BoundedText, TurnExecutionError> {
    BoundedText::try_new(value.trim().to_owned(), field, maximum)
        .map_err(|_| invariant("model_output_invalid", format!("{field} exceeds its bound")))
}

fn invariant(code: &'static str, message: impl Into<String>) -> TurnExecutionError {
    TurnExecutionError::new(
        crate::core::turn_error::TurnFailureKind::InvariantViolation,
        code,
        Some(TurnStage::Validation),
        message.into(),
    )
}
