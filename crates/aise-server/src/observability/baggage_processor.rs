use opentelemetry::baggage::BaggageExt;
use opentelemetry::trace::Span as _;
use opentelemetry::{Context, KeyValue};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::error::OTelSdkResult;
use opentelemetry_sdk::trace::{Span, SpanData, SpanProcessor};
use std::time::Duration;

pub const BAGGAGE_ALLOWLIST: &[&str] = &[
    "aise.trace.name",
    "aise.trace.tags",
    "aise.trace.environment",
    "aise.trace.release",
    "aise.schema.version",
    "langfuse.session.id",
    "aise.trace.metadata.story_id",
    "aise.trace.metadata.idempotency_key_digest",
    "aise.trace.metadata.turn_number",
];

#[derive(Debug)]
pub struct TraceAttributePropagationProcessor<P> {
    inner: P,
}

impl<P: SpanProcessor> TraceAttributePropagationProcessor<P> {
    pub fn new(inner: P) -> Self {
        Self { inner }
    }

    pub fn inner(&self) -> &P {
        &self.inner
    }
}

impl<P: SpanProcessor> SpanProcessor for TraceAttributePropagationProcessor<P> {
    fn on_start(&self, span: &mut Span, context: &Context) {
        let baggage = context.baggage();
        for key in BAGGAGE_ALLOWLIST {
            if let Some(value) = baggage.get(*key) {
                span.set_attribute(KeyValue::new(*key, value.as_str().to_owned()));
            }
        }
        self.inner.on_start(span, context);
    }

    fn on_end(&self, span: SpanData) {
        self.inner.on_end(span);
    }

    fn force_flush(&self) -> OTelSdkResult {
        self.inner.force_flush()
    }

    fn shutdown_with_timeout(&self, timeout: Duration) -> OTelSdkResult {
        self.inner.shutdown_with_timeout(timeout)
    }

    fn set_resource(&mut self, resource: &Resource) {
        self.inner.set_resource(resource);
    }
}

#[cfg(test)]
#[path = "tests/baggage_processor_tests.rs"]
mod tests;
