use crate::error::AiseError;
use crate::runtime::pipeline::TurnExecutionPipeline;
use crate::runtime::trace::{SpanPayload, ValidationData};
use crate::runtime::turn_execution_ctx::TurnExecutionContext;
use crate::validation::validation_model::ValidationResult;
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
    fn stage(&self) -> &'static str {
        "validation"
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), AiseError> {
        let mut result = {
            let pending = ctx.trace.begin_span("aise.validation", "schema.validate");
            let outcome = self.schema.validate(ctx);
            let payload = validation_payload(&outcome);
            ctx.trace.end_span_with(pending, &payload);
            outcome?
        };
        if result.pass {
            let pending = ctx.trace.begin_span("aise.validation", "consistency.validate");
            let outcome = self.consistency.validate(ctx).await;
            let payload = validation_payload(&outcome);
            ctx.trace.end_span_with(pending, &payload);
            result = outcome?;
        }
        ctx.validation = result;
        Ok(())
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
