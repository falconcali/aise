use super::*;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Metadata, Subscriber};

#[tokio::test]
async fn observe_result_preserves_the_original_success() {
    let result = observe_result(
        ObservationStep::ValidateRequest,
        ObservationFields::default(),
        async { Ok::<_, TestError>(41) },
        |_| unreachable!(),
    )
    .await;

    assert_eq!(result, Ok(41));
}

#[tokio::test]
async fn observe_result_preserves_the_original_error() {
    let original = TestError("business");
    let result = observe_result(
        ObservationStep::ValidateRequest,
        ObservationFields::default(),
        async { Err::<(), _>(original.clone()) },
        |error| super::super::fields::ObservationError {
            code: "test_error".to_owned(),
            failure_kind: "test".to_owned(),
            stage: Some("validation".to_owned()),
            message: error.0.to_owned(),
        },
    )
    .await;

    assert_eq!(result, Err(original));
}

#[test]
fn dropped_unfinished_span_records_incomplete() {
    let records = Arc::new(Mutex::new(Vec::new()));
    let subscriber = TestSubscriber::new(records.clone());
    let _guard = tracing::subscriber::set_default(subscriber);

    drop(ObservationSpan::begin(
        ObservationStep::ValidateRequest,
        ObservationFields::default(),
    ));

    assert!(
        records
            .lock()
            .unwrap()
            .iter()
            .any(|record| { record.field == "otel.status_message" && record.value == "incomplete" })
    );
}

#[test]
fn explicit_finish_does_not_record_incomplete() {
    let records = Arc::new(Mutex::new(Vec::new()));
    let subscriber = TestSubscriber::new(records.clone());
    let _guard = tracing::subscriber::set_default(subscriber);
    let span = ObservationSpan::begin(ObservationStep::ValidateRequest, ObservationFields::default());

    span.finish(ObservationFinish {
        status: ObservationStatus::Ok,
        ..ObservationFinish::default()
    });

    assert!(!records.lock().unwrap().iter().any(|record| record.value == "incomplete"));
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TestError(&'static str);

#[derive(Clone)]
struct CapturedRecord {
    field: String,
    value: String,
}

struct TestSubscriber {
    next_id: AtomicU64,
    records: Arc<Mutex<Vec<CapturedRecord>>>,
}

impl TestSubscriber {
    fn new(records: Arc<Mutex<Vec<CapturedRecord>>>) -> Self {
        Self {
            next_id: AtomicU64::new(1),
            records,
        }
    }
}

impl Subscriber for TestSubscriber {
    fn enabled(&self, _: &Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _: &Attributes<'_>) -> Id {
        Id::from_u64(self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    fn record(&self, _: &Id, values: &Record<'_>) {
        values.record(&mut RecordVisitor {
            records: self.records.clone(),
        });
    }

    fn record_follows_from(&self, _: &Id, _: &Id) {}

    fn event(&self, _: &Event<'_>) {}

    fn enter(&self, _: &Id) {}

    fn exit(&self, _: &Id) {}
}

struct RecordVisitor {
    records: Arc<Mutex<Vec<CapturedRecord>>>,
}

impl Visit for RecordVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.records.lock().unwrap().push(CapturedRecord {
            field: field.name().to_owned(),
            value: value.to_owned(),
        });
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.records.lock().unwrap().push(CapturedRecord {
            field: field.name().to_owned(),
            value: format!("{value:?}"),
        });
    }
}
