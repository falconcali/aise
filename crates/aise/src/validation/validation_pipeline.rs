use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::core::turn_trace::{SpanPayload, ValidationData};
use crate::core::turn_validation::{
    StateChange, ValidatedChangeSet, ValidationDecision, ValidationIssue, ValidationResult, fatal,
};
use crate::domain::ids::EventId;
use crate::domain::narrative::StoryEvent;
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
    if !proposal.character_changes.is_empty()
        || !proposal.world_change.add_facts.is_empty()
        || !proposal.memory_changes.is_empty()
    {
        return Err(fatal(
            "unsupported_change_kind",
            "character, world, and memory changes are not supported yet",
        ));
    }
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
    Ok(ValidatedChangeSet::new(
        proposal.story_text.clone(),
        events,
        Vec::new(),
        StateChange::Unchanged,
        Vec::new(),
        proposal.summary_delta.clone(),
    ))
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
