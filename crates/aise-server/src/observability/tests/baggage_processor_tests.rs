use super::*;
use opentelemetry::Context;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::error::OTelSdkResult;
use opentelemetry_sdk::trace::{Span, SpanData};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[derive(Debug)]
struct CountingProcessor {
    flushes: Arc<AtomicUsize>,
    shutdowns: Arc<AtomicUsize>,
}

impl SpanProcessor for CountingProcessor {
    fn on_start(&self, _span: &mut Span, _context: &Context) {}

    fn on_end(&self, _span: SpanData) {}

    fn force_flush(&self) -> OTelSdkResult {
        self.flushes.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn shutdown_with_timeout(&self, _timeout: Duration) -> OTelSdkResult {
        self.shutdowns.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn set_resource(&mut self, _resource: &Resource) {}
}

#[test]
fn baggage_allowlist_is_exact_and_bounded() {
    assert_eq!(
        BAGGAGE_ALLOWLIST,
        [
            "aise.trace.name",
            "aise.trace.tags",
            "aise.trace.environment",
            "aise.trace.release",
            "aise.schema.version",
            "langfuse.session.id",
            "aise.trace.metadata.story_id",
            "aise.trace.metadata.idempotency_key_digest",
            "aise.trace.metadata.turn_number",
        ]
    );
}

#[test]
fn lifecycle_operations_delegate_to_the_single_inner_processor() {
    let flushes = Arc::new(AtomicUsize::new(0));
    let shutdowns = Arc::new(AtomicUsize::new(0));
    let processor = TraceAttributePropagationProcessor::new(CountingProcessor {
        flushes: flushes.clone(),
        shutdowns: shutdowns.clone(),
    });

    assert!(processor.force_flush().is_ok());
    assert!(processor.shutdown_with_timeout(Duration::from_millis(10)).is_ok());
    assert_eq!(flushes.load(Ordering::SeqCst), 1);
    assert_eq!(shutdowns.load(Ordering::SeqCst), 1);
}
