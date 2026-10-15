use super::*;
use crate::core::{ChatMessage, TurnCancellation};
use crate::trace::{
    ContentCapture, ContentCapturePolicy, ObservabilityContentConfig, ObservationSession, SessionSpec, Trace, TraceSpec,
};
use async_trait::async_trait;
use futures::future::join_all;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::Notify;

struct DelayedProvider {
    active: AtomicUsize,
    calls: AtomicUsize,
    delay: Duration,
    entered: Notify,
    maximum_active: AtomicUsize,
}

#[async_trait]
impl LlmProvider for DelayedProvider {
    fn name(&self) -> &'static str {
        "delayed"
    }

    async fn complete(&self, _request: LlmCompletionRequest) -> Result<LlmCompletionResponse, LlmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.maximum_active.fetch_max(active, Ordering::SeqCst);
        self.entered.notify_one();
        tokio::time::sleep(self.delay).await;
        self.active.fetch_sub(1, Ordering::SeqCst);
        Ok(LlmCompletionResponse {
            content: "done".to_owned(),
            finish_reason: crate::llm::LlmCompletionFinishReason::Stop,
        })
    }
}

fn trace() -> Trace {
    let session = ObservationSession::begin(
        SessionSpec {
            id: None,
            user_id: None,
            metadata: Vec::new(),
        },
        ContentCapture::new(ObservabilityContentConfig {
            policy: ContentCapturePolicy::MetadataOnly,
            max_field_bytes: 1024,
            max_observation_bytes: 2048,
            detector_overlap_bytes: 0,
        }),
    );
    session.begin_trace(TraceSpec {
        name: "gateway-test",
        input: None,
        metadata: Vec::new(),
        tags: Vec::new(),
    })
}

fn spec() -> LlmCompletionSpec {
    LlmCompletionSpec {
        messages: vec![ChatMessage::user("test")],
    }
}

fn config(max_concurrent_calls: usize) -> LlmConfig {
    LlmConfig {
        max_concurrent_calls: NonZeroUsize::new(max_concurrent_calls).expect("nonzero limit"),
        timeout_ms: 5_000,
        ..LlmConfig::default()
    }
}

#[tokio::test]
async fn gateway_limits_active_provider_calls() {
    let provider = Arc::new(DelayedProvider {
        active: AtomicUsize::new(0),
        calls: AtomicUsize::new(0),
        delay: Duration::from_millis(20),
        entered: Notify::new(),
        maximum_active: AtomicUsize::new(0),
    });
    let gateway = LlmGateway::new(provider.clone(), config(2));
    let control = TurnControl::new(Instant::now() + Duration::from_secs(5), TurnCancellation::new());
    let trace = trace();

    let results = join_all((0..6).map(|_| gateway.complete(spec(), &control, trace.root()))).await;

    assert!(results.iter().all(Result::is_ok));
    assert_eq!(provider.calls.load(Ordering::SeqCst), 6);
    assert_eq!(provider.maximum_active.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn permit_wait_respects_turn_cancellation() {
    let provider = Arc::new(DelayedProvider {
        active: AtomicUsize::new(0),
        calls: AtomicUsize::new(0),
        delay: Duration::from_millis(50),
        entered: Notify::new(),
        maximum_active: AtomicUsize::new(0),
    });
    let gateway = LlmGateway::new(provider.clone(), config(1));
    let first_control = TurnControl::new(Instant::now() + Duration::from_secs(5), TurnCancellation::new());
    let cancellation = TurnCancellation::new();
    let second_control = TurnControl::new(Instant::now() + Duration::from_secs(5), cancellation.clone());
    let trace = trace();
    let first = gateway.complete(spec(), &first_control, trace.root());
    let second = async {
        provider.entered.notified().await;
        cancellation.cancel();
        gateway.complete(spec(), &second_control, trace.root()).await
    };

    let (first_result, second_result) = tokio::join!(first, second);

    assert!(first_result.is_ok());
    assert!(matches!(second_result, Err(LlmError::Cancelled)));
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn permit_wait_respects_turn_deadline() {
    let provider = Arc::new(DelayedProvider {
        active: AtomicUsize::new(0),
        calls: AtomicUsize::new(0),
        delay: Duration::from_millis(50),
        entered: Notify::new(),
        maximum_active: AtomicUsize::new(0),
    });
    let gateway = LlmGateway::new(provider.clone(), config(1));
    let first_control = TurnControl::new(Instant::now() + Duration::from_secs(5), TurnCancellation::new());
    let second_control = TurnControl::new(Instant::now() + Duration::from_millis(5), TurnCancellation::new());
    let trace = trace();
    let first = gateway.complete(spec(), &first_control, trace.root());
    let second = async {
        provider.entered.notified().await;
        gateway.complete(spec(), &second_control, trace.root()).await
    };

    let (first_result, second_result) = tokio::join!(first, second);

    assert!(first_result.is_ok());
    assert!(matches!(second_result, Err(LlmError::Timeout)));
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn limiter_closed_error_has_stable_kind() {
    assert_eq!(LlmConfig::default().max_concurrent_calls.get(), 16);
    assert_eq!(LlmError::ConcurrencyLimiterClosed.kind(), "concurrency_limiter_closed");
}
