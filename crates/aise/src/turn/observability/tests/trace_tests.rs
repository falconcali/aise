use super::*;
use opentelemetry::baggage::BaggageExt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Instrument, Metadata, Subscriber};

#[test]
fn late_bindings_update_the_current_baggage() {
    let subscriber = TestSubscriber::default();
    let _guard = tracing::subscriber::set_default(subscriber);
    let mut trace = ObservationTrace::begin(ObservationFields::default());

    trace.bind_session("session-1", "story-1");
    trace.bind_request("digest-1");
    trace.bind_turn(7);

    let baggage = trace.context().baggage();
    assert_eq!(baggage.get(SESSION_ID).map(ToString::to_string).as_deref(), Some("session-1"));
    assert_eq!(
        baggage.get(TRACE_METADATA_STORY_ID).map(ToString::to_string).as_deref(),
        Some("story-1")
    );
    assert_eq!(
        baggage
            .get(TRACE_METADATA_IDEMPOTENCY_KEY_DIGEST)
            .map(ToString::to_string)
            .as_deref(),
        Some("digest-1")
    );
    assert_eq!(
        baggage.get(TRACE_METADATA_TURN_NUMBER).map(ToString::to_string).as_deref(),
        Some("7")
    );
}

#[test]
fn repeated_binding_replaces_the_previous_baggage_value() {
    let subscriber = TestSubscriber::default();
    let _guard = tracing::subscriber::set_default(subscriber);
    let mut trace = ObservationTrace::begin(ObservationFields::default());

    trace.bind_request("digest-1");
    trace.bind_request("digest-2");

    assert_eq!(
        trace
            .context()
            .baggage()
            .get(TRACE_METADATA_IDEMPOTENCY_KEY_DIGEST)
            .map(ToString::to_string)
            .as_deref(),
        Some("digest-2")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn root_span_can_parent_work_in_a_spawned_task() {
    let created = Arc::new(Mutex::new(Vec::new()));
    let subscriber = TestSubscriber::new(created.clone());
    let _guard = tracing::subscriber::set_default(subscriber);
    let trace = ObservationTrace::begin(ObservationFields::default());
    let root_span = trace.span();

    tokio::spawn(
        async {
            drop(ObservationSpan::begin(
                ObservationStep::ValidateRequest,
                ObservationFields::default(),
            ));
        }
        .instrument(root_span),
    )
    .await
    .unwrap();

    let created = created.lock().unwrap();
    let root = created
        .iter()
        .find(|span| span.name == ObservationStep::ExecuteStoryTurn.name())
        .unwrap();
    let child = created
        .iter()
        .find(|span| span.name == ObservationStep::ValidateRequest.name())
        .unwrap();
    assert_eq!(child.parent, Some(root.id));
}

#[derive(Clone)]
struct CreatedSpan {
    id: u64,
    name: &'static str,
    parent: Option<u64>,
}

struct TestSubscriber {
    next_id: AtomicU64,
    current: Mutex<Vec<u64>>,
    created: Arc<Mutex<Vec<CreatedSpan>>>,
}

impl TestSubscriber {
    fn new(created: Arc<Mutex<Vec<CreatedSpan>>>) -> Self {
        Self {
            next_id: AtomicU64::new(1),
            current: Mutex::new(Vec::new()),
            created,
        }
    }
}

impl Default for TestSubscriber {
    fn default() -> Self {
        Self::new(Arc::new(Mutex::new(Vec::new())))
    }
}

impl Subscriber for TestSubscriber {
    fn enabled(&self, _: &Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, attributes: &Attributes<'_>) -> Id {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let parent = attributes
            .parent()
            .map(Id::into_u64)
            .or_else(|| self.current.lock().unwrap().last().copied());
        self.created.lock().unwrap().push(CreatedSpan {
            id,
            name: attributes.metadata().name(),
            parent,
        });
        Id::from_u64(id)
    }

    fn record(&self, _: &Id, _: &Record<'_>) {}

    fn record_follows_from(&self, _: &Id, _: &Id) {}

    fn event(&self, _: &Event<'_>) {}

    fn enter(&self, id: &Id) {
        self.current.lock().unwrap().push(id.clone().into_u64());
    }

    fn exit(&self, id: &Id) {
        let mut current = self.current.lock().unwrap();
        if current.last().copied() == Some(id.clone().into_u64()) {
            current.pop();
        }
    }
}
