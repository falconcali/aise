use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::core::turn_trace::{SpanPayload, ValidationData};
use crate::core::turn_validation::ValidationResult;
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
        if result.pass {
            let pending = ctx.trace().begin_span("aise.validation", "consistency.validate");
            let outcome = self.consistency.validate(ctx).await;
            let payload = validation_payload(&outcome);
            ctx.trace().end_span_with(pending, &payload);
            result = outcome?;
        }
        ctx.set_validation_result(result)
    }
}

fn validation_payload(outcome: &Result<ValidationResult, AiseError>) -> SpanPayload {
    match outcome {
        Ok(result) => SpanPayload::Validation(ValidationData {
            pass: result.pass,
            issues: result
                .issues
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
