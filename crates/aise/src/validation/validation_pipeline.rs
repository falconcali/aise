use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::TurnId;
use crate::domain::knowledge::fact::WorldFact;
use crate::domain::knowledge::memory::MemoryEntry;
use crate::domain::knowledge::query::{KnowledgeSource, allocate_knowledge_ids};
use crate::domain::knowledge::rumor::SharedRumor;
use crate::domain::knowledge::{KnowledgeEntry, KnowledgeSourceId};
use crate::domain::narrative::{EventKind, StoryEvent};
use crate::domain::narrative_graph::resolver::{NarrativeResolutionInput, NarrativeResolver};
use crate::domain::story_instance::role::StoryRoleState;
use crate::domain::story_instance::state::{RelationshipKey, RelationshipState};
use crate::domain::turn::{ProposedKnowledgeMutation, ProposedKnowledgeValue, ValidatedNarrativeResolution};
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_error::TurnExecutionError;
use crate::turn::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::turn::turn_trace::{SpanPayload, ValidationData};
use crate::turn::turn_validation::{
    RelationshipStateChange, RoleStateChange, StateChange, ValidatedChangeSet, ValidatedChangeSetParts,
    ValidatedKnowledgeMutation, ValidatedKnowledgeOperation, ValidationIssue, ValidationResult,
};
use crate::validation::narrative_candidate_state::CandidateNarrativeStateView;
use crate::validation::validators::DeterministicValidator;
use crate::validation::validators::changed_only::ChangedOnlyValidator;
use crate::validation::validators::domain_invariant::DomainInvariantValidator;
use crate::validation::validators::extraction_schema::ExtractionSchemaValidator;
use crate::validation::validators::reference::ReferenceValidator;
use crate::validation::validators::story_bounds::StoryBoundsValidator;
use crate::validation::validators::story_state_consistency::StoryStateConsistencyValidator;
use async_trait::async_trait;

#[derive(Default)]
pub struct ValidationPipeline {
    story_bounds: StoryBoundsValidator,
    extraction_schema: ExtractionSchemaValidator,
    reference: ReferenceValidator,
    domain_invariant: DomainInvariantValidator,
    changed_only: ChangedOnlyValidator,
    story_state_consistency: StoryStateConsistencyValidator,
}

#[async_trait]
impl TurnExecutionPipeline for ValidationPipeline {
    fn stage(&self) -> TurnStage {
        TurnStage::Validation
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), TurnExecutionError> {
        if let Some(extraction_version) = ctx.extraction_story_version() {
            if extraction_version != ctx.story_version() {
                return Err(TurnExecutionError::stale_state_extraction(Some(TurnStage::Validation)));
            }
        }
        let issues = self.run_deterministic(ctx)?;
        let payload = SpanPayload::Validation(ValidationData {
            pass: issues.is_empty(),
            issues: issues
                .iter()
                .map(|issue| format!("{}: {}", issue.code.as_str(), issue.message))
                .collect(),
        });
        let pending = ctx.trace().begin_span("aise.validation", "validation.execute");
        ctx.trace().end_span_with(pending, &payload);
        if issues.is_empty() {
            let change_set = build_change_set(ctx)?;
            return ctx.set_validation_result(ValidationResult::pass(change_set));
        }
        let result = ValidationResult::from_issues(issues, ctx.budget().max_validation_issues())?;
        ctx.set_validation_result(result)
    }
}

impl ValidationPipeline {
    fn run_deterministic(&self, ctx: &TurnExecutionContext) -> Result<Vec<ValidationIssue>, TurnExecutionError> {
        let mut issues = Vec::new();
        for validator in [
            &self.story_bounds as &dyn DeterministicValidator,
            &self.extraction_schema,
            &self.reference,
            &self.domain_invariant,
            &self.changed_only,
            &self.story_state_consistency,
        ] {
            issues.extend(validator.validate(ctx)?);
        }
        Ok(issues)
    }
}

fn build_change_set(ctx: &TurnExecutionContext) -> Result<ValidatedChangeSet, TurnExecutionError> {
    let story = ctx
        .story()
        .ok_or_else(|| invariant("missing_story", "no candidate story produced"))?;
    let extraction = ctx
        .extraction()
        .ok_or_else(|| invariant("missing_extraction", "no state extraction produced"))?;
    let extraction_envelope = ctx
        .extraction_envelope()
        .ok_or_else(|| invariant("missing_extraction", "no state extraction produced"))?;
    let snapshot = ctx
        .snapshot()
        .ok_or_else(|| invariant("missing_snapshot", "no story snapshot available"))?;
    let projection = ctx
        .narrative_projection()
        .ok_or_else(|| invariant("missing_narrative_projection", "no narrative projection available"))?;
    let turn_id = ctx.turn_id().clone();
    let current_turn = snapshot.base_revision().get().saturating_add(1);

    let story_text = bounded(story.story_text.as_str(), "story_text", ctx.budget().max_story_text_bytes())?;

    let role_changes = extraction
        .role_states
        .iter()
        .map(|state| {
            let current = snapshot
                .role(&state.role_id)
                .ok_or_else(|| invariant("role_change_reference", "role_id is not a known role"))?;
            let goals = state
                .goals
                .iter()
                .map(|goal| bounded(goal.as_str(), "role_goal", ctx.budget().max_item_bytes()))
                .collect::<Result<Vec<_>, _>>()?;
            let new_state = StoryRoleState {
                location: state.location.clone(),
                goals,
                attributes: state.attributes.clone(),
            };
            let mut updated = current.clone();
            updated.state = new_state.clone();
            let role_bytes = serde_json::to_vec(&updated)
                .map_err(|_| invariant("role_serialization_failed", "role state could not be serialized"))?
                .len();
            if role_bytes > ctx.budget().max_role_bytes() {
                return Err(invariant("role_bytes_exceeded", "role state exceeds max_role_bytes"));
            }
            Ok(RoleStateChange {
                role_id: state.role_id.clone(),
                new_state,
            })
        })
        .collect::<Result<Vec<_>, TurnExecutionError>>()?;

    let relationship_changes = extraction
        .relationship_states
        .iter()
        .map(|relationship| {
            let key = RelationshipKey {
                source_role_id: relationship.source_role_id.clone(),
                target_role_id: relationship.target_role_id.clone(),
                kind: relationship.kind.clone(),
            };
            RelationshipStateChange {
                key,
                new_state: RelationshipState {
                    source_role_id: relationship.source_role_id.clone(),
                    target_role_id: relationship.target_role_id.clone(),
                    kind: relationship.kind.clone(),
                    trust: relationship.trust,
                },
            }
        })
        .collect::<Vec<_>>();

    let knowledge_add_kinds = extraction
        .knowledge_changes
        .iter()
        .filter_map(|mutation| match mutation {
            ProposedKnowledgeMutation::Add { value } => Some(value.kind()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let allocation = allocate_knowledge_ids(snapshot.knowledge_id_high_water(), &knowledge_add_kinds)
        .map_err(|_| invariant("knowledge_id_allocation_overflow", "knowledge id allocation overflowed"))?;
    let mut assigned_ids = allocation.assigned.into_iter();
    let knowledge_mutations = extraction
        .knowledge_changes
        .iter()
        .enumerate()
        .map(|(index, mutation)| {
            let ordinal = u32::try_from(index)
                .map_err(|_| invariant("knowledge_count_overflow", "knowledge change count exceeds u32"))?;
            let operation = make_knowledge_operation(
                mutation,
                &mut assigned_ids,
                &turn_id,
                ctx.budget().max_knowledge_change_bytes(),
                ctx.identity().started_at_ms(),
            )?;
            Ok(ValidatedKnowledgeMutation { ordinal, operation })
        })
        .collect::<Result<Vec<_>, TurnExecutionError>>()?;
    let knowledge_id_high_water = allocation.new_high_water;

    let mut narrative_events = Vec::new();
    for intent in &projection.plan.world_event_intents {
        let index = narrative_events.len();
        narrative_events.push(make_event(
            &turn_id,
            index,
            EventKind::WorldChange,
            serde_json::json!({
                "event_key": intent.event_key,
                "category": intent.category,
                "participants": intent.participants,
                "location": intent.location,
                "description": intent.description,
            }),
        )?);
    }

    let candidate_view = CandidateNarrativeStateView::new(snapshot, &role_changes, &relationship_changes);
    let resolver = NarrativeResolver::new(ctx.budget().narrative_limits());
    let resolution = resolver
        .resolve(NarrativeResolutionInput {
            definition: snapshot.narrative_definition(),
            state: snapshot.narrative_state(),
            candidate_view: &candidate_view,
            extraction: extraction_envelope,
            current_turn,
        })
        .map_err(|error| invariant("narrative_resolution_failed", error.to_string()))?;

    let delivered_effect_ids = projection
        .plan
        .effect_dispositions
        .iter()
        .map(|disposition| match disposition {
            crate::domain::narrative_graph::projector::NarrativeEffectDisposition::PendingDelivery { effect_id }
            | crate::domain::narrative_graph::projector::NarrativeEffectDisposition::NotApplicable {
                effect_id, ..
            } => effect_id.clone(),
        })
        .collect::<std::collections::BTreeSet<_>>();

    let mut pending_effects = snapshot
        .narrative_state()
        .pending_effects
        .values()
        .filter(|pending| !delivered_effect_ids.contains(&pending.effect_id))
        .cloned()
        .collect::<Vec<_>>();
    pending_effects.extend(resolution.pending_effects);

    let narrative_resolution = ValidatedNarrativeResolution {
        candidate_version: extraction_envelope.candidate_version.clone(),
        transitions: resolution.transitions,
        condition_results: resolution.condition_results,
        pending_effects,
        next_graph_revision: resolution.next_graph_revision,
    };

    let mut final_nodes = snapshot.narrative_state().node_states.clone();
    for transition in &narrative_resolution.transitions {
        let state = match transition.kind {
            crate::domain::narrative_graph::effect::NarrativeTransitionKind::Activate => {
                crate::domain::narrative_graph::condition::NarrativeNodeState::Active
            }
            crate::domain::narrative_graph::effect::NarrativeTransitionKind::Complete => {
                crate::domain::narrative_graph::condition::NarrativeNodeState::Completed
            }
            crate::domain::narrative_graph::effect::NarrativeTransitionKind::Skip => {
                crate::domain::narrative_graph::condition::NarrativeNodeState::Skipped
            }
        };
        final_nodes.insert(transition.node_key.clone(), state);
    }
    let committed_sequence = snapshot
        .story_continuity()
        .next_sequence()
        .map_err(|_| invariant("story_sequence_overflow", "failed to assign the committed story sequence"))?;
    let constraints = snapshot
        .active_constraints()
        .iter()
        .filter(|constraint| match &constraint.lifecycle {
            crate::domain::asset::constraint::StoryConstraintLifecycle::Persistent => true,
            crate::domain::asset::constraint::StoryConstraintLifecycle::ThroughSequence { sequence } => {
                sequence.get() > committed_sequence.get()
            }
            crate::domain::asset::constraint::StoryConstraintLifecycle::UntilNarrativeNodeResolved { node_key } => {
                !matches!(
                    final_nodes.get(node_key),
                    Some(
                        crate::domain::narrative_graph::condition::NarrativeNodeState::Completed
                            | crate::domain::narrative_graph::condition::NarrativeNodeState::Skipped
                    )
                )
            }
        })
        .cloned()
        .collect::<Vec<_>>();
    let constraint_change = if constraints == snapshot.active_constraints() {
        StateChange::Unchanged
    } else {
        StateChange::Replace(constraints)
    };

    ValidatedChangeSet::new(ValidatedChangeSetParts {
        story_text,
        role_changes,
        relationship_changes,
        knowledge_mutations,
        knowledge_id_high_water,
        narrative_events,
        narrative_resolution,
        constraint_change,
    })
}

fn make_event(
    turn_id: &TurnId,
    index: usize,
    kind: EventKind,
    payload: serde_json::Value,
) -> Result<StoryEvent, TurnExecutionError> {
    Ok(StoryEvent {
        id: crate::domain::ids::EventId::from(format!("{}:event:{index}", turn_id.as_str())),
        turn_id: turn_id.clone(),
        seq: u32::try_from(index).map_err(|_| invariant("event_count_overflow", "event count exceeds u32"))?,
        kind,
        payload,
    })
}

fn make_knowledge_operation(
    mutation: &ProposedKnowledgeMutation,
    assigned_ids: &mut impl Iterator<Item = KnowledgeSourceId>,
    turn_id: &TurnId,
    max_bytes: usize,
    created_at_ms: i64,
) -> Result<ValidatedKnowledgeOperation, TurnExecutionError> {
    match mutation {
        ProposedKnowledgeMutation::Add { value } => {
            let source_id = assigned_ids
                .next()
                .ok_or_else(|| invariant("knowledge_id_allocation_missing", "knowledge id allocation ran out"))?;
            let entry = make_knowledge_entry(value, source_id, turn_id, max_bytes, created_at_ms)?;
            Ok(ValidatedKnowledgeOperation::Add(entry))
        }
        ProposedKnowledgeMutation::Update { target, value } => {
            let entry = make_knowledge_entry(value, target.clone(), turn_id, max_bytes, created_at_ms)?;
            Ok(ValidatedKnowledgeOperation::Update {
                target: target.clone(),
                value: entry,
            })
        }
        ProposedKnowledgeMutation::Delete { target } => {
            Ok(ValidatedKnowledgeOperation::Delete { target: target.clone() })
        }
    }
}

fn make_knowledge_entry(
    value: &ProposedKnowledgeValue,
    source_id: KnowledgeSourceId,
    turn_id: &TurnId,
    max_bytes: usize,
    created_at_ms: i64,
) -> Result<KnowledgeEntry, TurnExecutionError> {
    let source = KnowledgeSource::CommittedTurn {
        turn_id: turn_id.clone(),
    };
    match (value, source_id) {
        (
            ProposedKnowledgeValue::Fact {
                content,
                proposition,
                retrieval_hint,
                entities,
                topics,
                salience,
            },
            KnowledgeSourceId::Fact(id),
        ) => Ok(KnowledgeEntry::Fact(WorldFact {
            id,
            key: None,
            text: bounded(content.as_str(), "fact_content", max_bytes)?,
            proposition: proposition.clone(),
            retrieval_hint: retrieval_hint.clone(),
            entities: canonical(entities.clone()),
            topics: canonical(topics.clone()),
            salience: *salience,
            source,
        })),
        (
            ProposedKnowledgeValue::Rumor {
                content,
                claim,
                retrieval_hint,
                entities,
                topics,
                salience,
                source_role_id,
                truth_value,
            },
            KnowledgeSourceId::Rumor(id),
        ) => Ok(KnowledgeEntry::Rumor(SharedRumor {
            id,
            key: None,
            content: bounded(content.as_str(), "rumor_content", max_bytes)?,
            claim: claim.clone(),
            retrieval_hint: retrieval_hint.clone(),
            entities: canonical(entities.clone()),
            topics: canonical(topics.clone()),
            salience: *salience,
            source_role_id: source_role_id.clone(),
            truth_value: truth_value.clone(),
            source,
        })),
        (
            ProposedKnowledgeValue::Memory {
                owner,
                memory_kind,
                content,
                entities,
                topics,
                salience,
            },
            KnowledgeSourceId::Memory(id),
        ) => {
            let mut entities = entities.clone();
            entities.push(KnowledgeEntity::Role(owner.clone()));
            Ok(KnowledgeEntry::Memory(MemoryEntry {
                id,
                owner: owner.clone(),
                kind: memory_kind.clone(),
                content: bounded(content.as_str(), "memory_content", max_bytes)?,
                entities: canonical(entities),
                topics: canonical(topics.clone()),
                salience: *salience,
                source,
                created_at_ms,
            }))
        }
        _ => Err(invariant(
            "knowledge_kind_mismatch",
            "knowledge target and value kind do not match",
        )),
    }
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
        crate::turn::turn_error::TurnFailureKind::InvariantViolation,
        code,
        Some(TurnStage::Validation),
        message.into(),
    )
}
