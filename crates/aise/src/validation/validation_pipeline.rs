use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::{EventId, RoleIdHighWater, StoryId, TurnNumber};
use crate::domain::narrative::{EventKind, StoryEvent};
use crate::domain::narrative_graph::resolver::{NarrativeResolutionInput, NarrativeResolver};
use crate::domain::story_instance::role::{StoryRole, StoryRoleState};
use crate::domain::story_instance::state::{RelationshipKey, RelationshipState};
use crate::domain::turn::{
    ExtractionEnrichmentError, KnowledgeEnrichmentContext, ValidatedNarrativeResolution, enrich_extracted_knowledge,
};
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_error::TurnExecutionError;
use crate::turn::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::turn::turn_trace::{SpanPayload, ValidationData};
use crate::turn::turn_validation::{
    RoleStateChange, StateChange, ValidatedChangeSet, ValidatedChangeSetParts, ValidatedRelationshipOperation,
    ValidationIssue, ValidationResult,
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
    let story_id = ctx.story_id().clone();
    let turn_number = ctx.turn_number();
    let current_turn = snapshot.base_revision().get().saturating_add(1);
    let dto = extraction;

    let story_text = bounded(story.story_text.as_str(), "story_text", ctx.budget().max_story_text_bytes())?;

    let new_roles = dto
        .new_roles
        .iter()
        .map(|role| {
            let role_id = crate::domain::ids::RoleId::try_new(role.role_id.clone())
                .map_err(|_| invariant("new_role_id_invalid", "new role id does not resolve"))?;
            let location = crate::domain::asset::ids::LocationKey::try_new(role.location.clone())
                .map_err(|_| invariant("new_role_location_invalid", "new role location does not resolve"))?;
            let goals = role
                .goals
                .iter()
                .map(|goal| bounded(goal, "role_goal", ctx.budget().max_item_bytes()))
                .collect::<Result<Vec<_>, _>>()?;
            let profile_bytes = ctx.budget().state_extraction_limits().max_role_profile_bytes;
            Ok(StoryRole {
                role_id,
                controller: crate::domain::story_instance::role::RoleController::Ai,
                role_label: bounded(&role.role_label, "role_label", profile_bytes)?,
                narrative_function: bounded(&role.narrative_function, "narrative_function", profile_bytes)?,
                background: optional_bounded(&role.background, "background", profile_bytes)?,
                effective_profile: crate::domain::asset::character_card::CharacterProfile {
                    name: bounded(&role.name, "name", profile_bytes)?,
                    appearance: optional_bounded(&role.appearance, "appearance", profile_bytes)?,
                    personality: optional_bounded(&role.personality, "personality", profile_bytes)?,
                    speaking_style: optional_bounded(&role.speaking_style, "speaking_style", profile_bytes)?,
                    dialogue_examples: Vec::new(),
                },
                source_character: None,
                state: StoryRoleState {
                    location,
                    goals,
                    attributes: role.attributes.clone().into_iter().map(|(k, v)| (k.into(), v)).collect(),
                },
            })
        })
        .collect::<Result<Vec<_>, TurnExecutionError>>()?;

    let role_changes = dto
        .role_states
        .iter()
        .map(|state| {
            let role_id = crate::domain::ids::RoleId::try_new(state.role_id.clone())
                .map_err(|_| invariant("role_change_reference", "role_id is not a known role"))?;
            let location = crate::domain::asset::ids::LocationKey::try_new(state.location.clone())
                .map_err(|_| invariant("role_change_location_invalid", "role location does not resolve"))?;
            let goals = state
                .goals
                .iter()
                .map(|goal| bounded(goal, "role_goal", ctx.budget().max_item_bytes()))
                .collect::<Result<Vec<_>, _>>()?;
            let new_state = StoryRoleState {
                location,
                goals,
                attributes: state.attributes.clone().into_iter().map(|(k, v)| (k.into(), v)).collect(),
            };
            Ok(RoleStateChange { role_id, new_state })
        })
        .collect::<Result<Vec<_>, TurnExecutionError>>()?;

    let relationship_operations = dto
        .relationship_states
        .iter()
        .map(|relationship| {
            let source_role_id = crate::domain::ids::RoleId::try_new(relationship.source_role_id.clone())
                .map_err(|_| invariant("relationship_reference_invalid", "source_role_id does not resolve"))?;
            let target_role_id = crate::domain::ids::RoleId::try_new(relationship.target_role_id.clone())
                .map_err(|_| invariant("relationship_reference_invalid", "target_role_id does not resolve"))?;
            let kind = crate::domain::asset::ids::RelationshipKind::try_new(relationship.kind.clone())
                .map_err(|_| invariant("relationship_kind_invalid", "relationship kind does not resolve"))?;
            let trust = i16::try_from(relationship.trust)
                .map_err(|_| invariant("relationship_trust_out_of_range", "relationship trust is out of range"))?;
            let key = RelationshipKey {
                source_role_id: source_role_id.clone(),
                target_role_id: target_role_id.clone(),
                kind: kind.clone(),
            };
            let new_state = RelationshipState {
                source_role_id,
                target_role_id,
                kind,
                trust,
            };
            let existing = snapshot.relationships().iter().any(|state| state.key() == key);
            Ok(if existing {
                ValidatedRelationshipOperation::Update(crate::turn::turn_validation::RelationshipStateChange {
                    key,
                    new_state,
                })
            } else {
                ValidatedRelationshipOperation::Add(new_state)
            })
        })
        .collect::<Result<Vec<_>, TurnExecutionError>>()?;

    let enrichment_context = KnowledgeEnrichmentContext {
        retrieved: ctx.retrieved(),
        turn_number,
        created_at_ms: ctx.identity().started_at_ms(),
        max_content_bytes: ctx.budget().max_knowledge_change_bytes(),
    };
    let (knowledge_mutations, knowledge_id_high_water) =
        enrich_extracted_knowledge(dto, snapshot, &new_roles, &enrichment_context)
            .map_err(enrichment_error_to_turn_error)?;
    let next_role_id_high_water =
        RoleIdHighWater::new(snapshot.role_id_high_water().get().saturating_add(new_roles.len() as u64));

    let mut narrative_events = Vec::new();
    for intent in &projection.plan.world_event_intents {
        let index = narrative_events.len();
        narrative_events.push(make_event(
            &story_id,
            turn_number,
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

    let candidate_view =
        CandidateNarrativeStateView::new(snapshot, &new_roles, &role_changes, &relationship_operations);
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
        new_roles,
        role_changes,
        relationship_operations,
        knowledge_mutations,
        knowledge_id_high_water,
        next_role_id_high_water,
        narrative_events,
        narrative_resolution,
        constraint_change,
    })
}

fn enrichment_error_to_turn_error(error: ExtractionEnrichmentError) -> TurnExecutionError {
    invariant("knowledge_enrichment_failed", error.to_string())
}

fn make_event(
    story_id: &StoryId,
    turn_number: TurnNumber,
    index: usize,
    kind: EventKind,
    payload: serde_json::Value,
) -> Result<StoryEvent, TurnExecutionError> {
    Ok(StoryEvent {
        id: EventId::from(format!("{story_id}:turn:{turn_number}:event:{index}")),
        turn_number,
        seq: u32::try_from(index).map_err(|_| invariant("event_count_overflow", "event count exceeds u32"))?,
        kind,
        payload,
    })
}

fn optional_bounded(
    value: &str,
    field: &'static str,
    maximum: usize,
) -> Result<Option<BoundedText>, TurnExecutionError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    bounded(trimmed, field, maximum).map(Some)
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
