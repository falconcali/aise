use aise::LlmConfig;
use aise::core::turn_budget::{TurnBudget, TurnBudgetLimits};
use aise::core::turn_context::TurnExecutionContext;
use aise::core::turn_contract::{IdempotencyKey, TurnCancellation, TurnControl, TurnIdentity, TurnRequest};
use aise::core::turn_pipeline::TurnStage;
use aise::core::turn_trace::TraceRecorder;
use aise::domain::ids::{StoryId, TurnId};
use aise::llm::LlmGateway;
use aise::llm::accounting::{FinishReason, LlmCompletion, LlmTokenUsage, UsageAccuracy};
use aise::llm::error::LlmError;
use aise::llm::message::{ChatMessage, CompletionRequest, CompletionSpec, EmbeddingOutput, EmbeddingRequest, Role};
use aise::llm::provider::{DeltaSink, LlmProvider};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::oneshot;

#[derive(Clone)]
struct MockProvider {
    active: Arc<AtomicUsize>,
    max_active: Arc<AtomicUsize>,
    calls: Arc<AtomicUsize>,
    block: Arc<Mutex<Option<oneshot::Receiver<()>>>>,
    fail: Arc<AtomicUsize>,
    usage: Arc<Mutex<Option<LlmTokenUsage>>>,
    delay: Arc<Mutex<Option<Duration>>>,
}

impl MockProvider {
    fn new() -> Self {
        Self {
            active: Arc::new(AtomicUsize::new(0)),
            max_active: Arc::new(AtomicUsize::new(0)),
            calls: Arc::new(AtomicUsize::new(0)),
            block: Arc::new(Mutex::new(None)),
            fail: Arc::new(AtomicUsize::new(0)),
            usage: Arc::new(Mutex::new(None)),
            delay: Arc::new(Mutex::new(None)),
        }
    }

    fn set_block(&self, rx: oneshot::Receiver<()>) {
        *self.block.lock().unwrap() = Some(rx);
    }

    fn set_fail(&self) {
        self.fail.store(1, Ordering::SeqCst);
    }

    fn set_usage(&self, usage: LlmTokenUsage) {
        *self.usage.lock().unwrap() = Some(usage);
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    fn max_active(&self) -> usize {
        self.max_active.load(Ordering::SeqCst)
    }

    async fn enter(&self) -> Result<(), ()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_active.fetch_max(active, Ordering::SeqCst);
        let blocker = self.block.lock().unwrap().take();
        if let Some(rx) = blocker {
            let _ = rx.await;
        }
        let delay = *self.delay.lock().unwrap();
        if let Some(delay) = delay {
            tokio::time::sleep(delay).await;
        }
        self.active.fetch_sub(1, Ordering::SeqCst);
        Ok(())
    }
}

#[async_trait::async_trait]
impl LlmProvider for MockProvider {
    fn provider_name(&self) -> &'static str {
        "mock"
    }

    async fn complete(&self, _req: &CompletionRequest) -> Result<LlmCompletion, LlmError> {
        self.enter().await.map_err(|_| LlmError::Cancelled)?;
        if self.fail.load(Ordering::SeqCst) == 1 {
            return Err(LlmError::ProviderRejected("mock rejected".into()));
        }
        let usage = self.usage.lock().unwrap().clone();
        Ok(LlmCompletion {
            text: "response text".to_string(),
            finish_reason: Some(FinishReason::Stop),
            usage,
            charge: None,
        })
    }

    async fn complete_stream(&self, _req: &CompletionRequest, _on_delta: DeltaSink) -> Result<LlmCompletion, LlmError> {
        self.enter().await.map_err(|_| LlmError::Cancelled)?;
        let usage = self.usage.lock().unwrap().clone();
        Ok(LlmCompletion {
            text: "response text".to_string(),
            finish_reason: Some(FinishReason::Stop),
            usage,
            charge: None,
        })
    }

    async fn embed(&self, _req: &EmbeddingRequest) -> Result<EmbeddingOutput, LlmError> {
        Err(LlmError::EmbeddingUnsupported)
    }
}

fn budget() -> TurnBudget {
    TurnBudget::new(TurnBudgetLimits {
        max_repair_rounds: 3,
        max_llm_calls: 16,
        max_input_tokens: 100_000,
        max_output_tokens: 2_048,
        max_total_tokens: 200_000,
        max_retrieved_items: 5,
    })
}

fn new_ctx(deadline: Instant) -> TurnExecutionContext {
    new_ctx_with_cancellation(deadline, TurnCancellation::new())
}

fn new_ctx_with_cancellation(deadline: Instant, cancellation: TurnCancellation) -> TurnExecutionContext {
    TurnExecutionContext::new(
        TurnIdentity::new(
            StoryId::from("story-1"),
            TurnId::from("turn-1"),
            IdempotencyKey::try_new("key-1".into()).unwrap(),
            1,
        )
        .unwrap(),
        TurnRequest::try_new("hello".into()).unwrap(),
        budget(),
        TurnControl::new(deadline, cancellation),
        TraceRecorder::new(),
    )
    .unwrap()
}

fn far_deadline() -> Instant {
    Instant::now() + Duration::from_secs(60)
}

fn spec() -> CompletionSpec {
    CompletionSpec {
        messages: vec![ChatMessage {
            role: Role::User,
            content: "hello".into(),
        }],
        max_output_tokens: 64,
        purpose: "test",
    }
}

fn gateway(provider: Arc<MockProvider>, config: LlmConfig) -> Arc<LlmGateway> {
    Arc::new(LlmGateway::new(provider, config).unwrap())
}

#[tokio::test]
async fn all_calls_wait_for_shared_permit() {
    let provider = MockProvider::new();
    let (tx, rx) = oneshot::channel();
    provider.set_block(rx);
    let config = LlmConfig {
        max_concurrent: 1,
        queue_timeout_ms: 2_000,
        ..LlmConfig::default()
    };
    let gateway = gateway(Arc::new(provider.clone()), config);

    let g1 = gateway.clone();
    let first = tokio::spawn(async move {
        let mut ctx1 = new_ctx(far_deadline());
        let scope1 = ctx1.llm_call_scope(TurnStage::StoryGenerator);
        g1.complete(scope1, spec()).await
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(provider.calls(), 1);
    assert_eq!(provider.max_active(), 1);

    let g2 = gateway.clone();
    let second = tokio::spawn(async move {
        let mut ctx2 = new_ctx(far_deadline());
        let scope2 = ctx2.llm_call_scope(TurnStage::StoryGenerator);
        g2.complete(scope2, spec()).await
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(provider.calls(), 1);
    assert_eq!(provider.max_active(), 1);

    tx.send(()).unwrap();
    let first_result = first.await.unwrap().unwrap();
    let second_result = second.await.unwrap().unwrap();
    assert_eq!(first_result.text, "response text");
    assert_eq!(second_result.text, "response text");
    assert_eq!(provider.max_active(), 1);
}

#[tokio::test]
async fn permit_wait_respects_queue_timeout() {
    let provider = MockProvider::new();
    let (tx, rx) = oneshot::channel();
    provider.set_block(rx);
    let config = LlmConfig {
        max_concurrent: 1,
        queue_timeout_ms: 100,
        ..LlmConfig::default()
    };
    let gateway = gateway(Arc::new(provider.clone()), config);

    let g1 = gateway.clone();
    let first = tokio::spawn(async move {
        let mut ctx1 = new_ctx(far_deadline());
        let scope1 = ctx1.llm_call_scope(TurnStage::StoryGenerator);
        g1.complete(scope1, spec()).await
    });
    tokio::time::sleep(Duration::from_millis(30)).await;

    let g2 = gateway.clone();
    let second = tokio::spawn(async move {
        let mut ctx2 = new_ctx(far_deadline());
        let scope2 = ctx2.llm_call_scope(TurnStage::StoryGenerator);
        g2.complete(scope2, spec()).await
    });
    let error = second.await.unwrap().unwrap_err();
    assert!(matches!(error, LlmError::QueueTimeout));

    tx.send(()).unwrap();
    let _ = first.await.unwrap().unwrap();
}

#[tokio::test]
async fn permit_wait_respects_turn_deadline() {
    let provider = MockProvider::new();
    let (tx, rx) = oneshot::channel();
    provider.set_block(rx);
    let config = LlmConfig {
        max_concurrent: 1,
        queue_timeout_ms: 5_000,
        ..LlmConfig::default()
    };
    let gateway = gateway(Arc::new(provider.clone()), config);

    let g1 = gateway.clone();
    let first = tokio::spawn(async move {
        let mut ctx1 = new_ctx(far_deadline());
        let scope1 = ctx1.llm_call_scope(TurnStage::StoryGenerator);
        g1.complete(scope1, spec()).await
    });
    tokio::time::sleep(Duration::from_millis(30)).await;

    let g2 = gateway.clone();
    let second = tokio::spawn(async move {
        let mut ctx2 = new_ctx(Instant::now() + Duration::from_millis(150));
        let scope2 = ctx2.llm_call_scope(TurnStage::StoryGenerator);
        g2.complete(scope2, spec()).await
    });
    let error = second.await.unwrap().unwrap_err();
    assert!(matches!(error, LlmError::TurnDeadlineExceeded));

    tx.send(()).unwrap();
    let _ = first.await.unwrap().unwrap();
}

#[tokio::test]
async fn provider_call_respects_cancellation() {
    let provider = MockProvider::new();
    let (tx, rx) = oneshot::channel();
    provider.set_block(rx);
    let config = LlmConfig {
        max_concurrent: 1,
        provider_timeout_ms: 5_000,
        ..LlmConfig::default()
    };
    let gateway = gateway(Arc::new(provider.clone()), config);

    let cancellation = TurnCancellation::new();
    let task_gateway = gateway.clone();
    let cancel_for_task = cancellation.clone();
    let task = tokio::spawn(async move {
        let mut ctx = new_ctx_with_cancellation(far_deadline(), cancel_for_task);
        let scope = ctx.llm_call_scope(TurnStage::StoryGenerator);
        task_gateway.complete(scope, spec()).await
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    cancellation.cancel();
    let error = task.await.unwrap().unwrap_err();
    drop(tx);
    assert!(matches!(error, LlmError::Cancelled));
}

#[tokio::test]
async fn stream_respects_cancellation() {
    let provider = MockProvider::new();
    let (tx, rx) = oneshot::channel();
    provider.set_block(rx);
    let config = LlmConfig {
        max_concurrent: 1,
        provider_timeout_ms: 5_000,
        ..LlmConfig::default()
    };
    let gateway = gateway(Arc::new(provider.clone()), config);

    let cancellation = TurnCancellation::new();
    let task_gateway = gateway.clone();
    let cancel_for_task = cancellation.clone();
    let sink: DeltaSink = Box::new(|_delta| {});
    let task = tokio::spawn(async move {
        let mut ctx = new_ctx_with_cancellation(far_deadline(), cancel_for_task);
        let scope = ctx.llm_call_scope(TurnStage::StoryGenerator);
        task_gateway.complete_stream(scope, spec(), sink).await
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    cancellation.cancel();
    let error = task.await.unwrap().unwrap_err();
    drop(tx);
    assert!(matches!(error, LlmError::Cancelled));
}

#[tokio::test]
async fn budget_is_reserved_before_provider_dispatch() {
    let provider = MockProvider::new();
    let config = LlmConfig::default();
    let gateway = gateway(Arc::new(provider.clone()), config);

    let tight = TurnBudget::new(TurnBudgetLimits {
        max_repair_rounds: 0,
        max_llm_calls: 16,
        max_input_tokens: 100_000,
        max_output_tokens: 2_048,
        max_total_tokens: 10,
        max_retrieved_items: 5,
    });
    let mut ctx = TurnExecutionContext::new(
        TurnIdentity::new(
            StoryId::from("story-1"),
            TurnId::from("turn-1"),
            IdempotencyKey::try_new("key-1".into()).unwrap(),
            1,
        )
        .unwrap(),
        TurnRequest::try_new("hello".into()).unwrap(),
        tight,
        TurnControl::new(far_deadline(), TurnCancellation::new()),
        TraceRecorder::new(),
    )
    .unwrap();
    let scope = ctx.llm_call_scope(TurnStage::StoryGenerator);
    let error = gateway.complete(scope, spec()).await.unwrap_err();
    assert!(matches!(error, LlmError::TokenBudgetExceeded(_)));
    assert_eq!(provider.calls(), 0);
}

#[tokio::test]
async fn actual_usage_settles_reserved_tokens() {
    let provider = MockProvider::new();
    provider.set_usage(LlmTokenUsage {
        input_tokens: 100,
        cached_input_tokens: None,
        output_tokens: 50,
        total_tokens: 150,
        accuracy: UsageAccuracy::Exact,
    });
    let gateway = gateway(Arc::new(provider.clone()), LlmConfig::default());

    let mut ctx = new_ctx(far_deadline());
    let scope = ctx.llm_call_scope(TurnStage::StoryGenerator);
    let completion = gateway.complete(scope, spec()).await.unwrap();
    assert_eq!(completion.usage.as_ref().map(|u| u.input_tokens), Some(100));
    assert_eq!(ctx.budget().llm_calls(), 1);
}

#[tokio::test]
async fn missing_provider_usage_is_marked_estimated() {
    let provider = MockProvider::new();
    let gateway = gateway(Arc::new(provider.clone()), LlmConfig::default());

    let mut ctx = new_ctx(far_deadline());
    let scope = ctx.llm_call_scope(TurnStage::StoryGenerator);
    let completion = gateway.complete(scope, spec()).await.unwrap();
    let usage = completion.usage.unwrap();
    assert_eq!(usage.accuracy, UsageAccuracy::Estimated);
    assert!(usage.input_tokens >= 1);
    assert!(usage.output_tokens >= 1);
}

#[tokio::test]
async fn pricing_uses_integer_units() {
    let provider = MockProvider::new();
    provider.set_usage(LlmTokenUsage {
        input_tokens: 1_000,
        cached_input_tokens: None,
        output_tokens: 2_000,
        total_tokens: 3_000,
        accuracy: UsageAccuracy::Exact,
    });
    let config = LlmConfig {
        price_input_per_1k_tokens: Some(10),
        price_output_per_1k_tokens: Some(20),
        ..LlmConfig::default()
    };
    let gateway = gateway(Arc::new(provider.clone()), config);

    let mut ctx = new_ctx(far_deadline());
    let scope = ctx.llm_call_scope(TurnStage::StoryGenerator);
    let completion = gateway.complete(scope, spec()).await.unwrap();
    let charge = completion.charge.unwrap();
    assert_eq!(charge.amount_minor, 50);
}

#[tokio::test]
async fn llm_trace_closes_on_success() {
    let provider = MockProvider::new();
    let gateway = gateway(Arc::new(provider.clone()), LlmConfig::default());

    let mut ctx = new_ctx(far_deadline());
    let scope = ctx.llm_call_scope(TurnStage::StoryGenerator);
    gateway.complete(scope, spec()).await.unwrap();
    let trace = ctx.trace().build(&StoryId::from("story-1"), &TurnId::from("turn-1"));
    let llm = trace.spans.iter().find(|s| s.kind == "aise.llm_call").expect("llm span");
    assert_eq!(llm.payload["status"], "ok");
    assert_eq!(llm.payload["error_kind"], serde_json::Value::Null);
}

#[tokio::test]
async fn llm_trace_closes_on_provider_error() {
    let provider = MockProvider::new();
    provider.set_fail();
    let gateway = gateway(Arc::new(provider.clone()), LlmConfig::default());

    let mut ctx = new_ctx(far_deadline());
    let scope = ctx.llm_call_scope(TurnStage::StoryGenerator);
    let error = gateway.complete(scope, spec()).await.unwrap_err();
    assert!(matches!(error, LlmError::ProviderRejected(_)));
    let trace = ctx.trace().build(&StoryId::from("story-1"), &TurnId::from("turn-1"));
    let llm = trace.spans.iter().find(|s| s.kind == "aise.llm_call").expect("llm span");
    assert_eq!(llm.payload["status"], "error");
    assert_eq!(llm.payload["error_kind"], "provider_rejected");
}

#[tokio::test]
async fn llm_trace_closes_on_timeout_and_cancel() {
    let provider = MockProvider::new();
    let (tx, rx) = oneshot::channel();
    provider.set_block(rx);
    let config = LlmConfig {
        provider_timeout_ms: 50,
        ..LlmConfig::default()
    };
    let gateway = gateway(Arc::new(provider.clone()), config);

    let mut ctx = new_ctx(far_deadline());
    let cancellation = ctx.control().cancellation().clone();
    let scope = ctx.llm_call_scope(TurnStage::StoryGenerator);
    let error = gateway.complete(scope, spec()).await.unwrap_err();
    assert!(matches!(error, LlmError::ProviderTimeout));
    let trace = ctx.trace().build(&StoryId::from("story-1"), &TurnId::from("turn-1"));
    let llm = trace.spans.iter().find(|s| s.kind == "aise.llm_call").expect("llm span");
    assert_eq!(llm.payload["status"], "error");
    assert_eq!(llm.payload["error_kind"], "provider_timeout");
    let _ = tx.send(());
    let _ = cancellation;
}

#[tokio::test]
async fn default_trace_does_not_store_prompt_or_response_text() {
    let provider = MockProvider::new();
    let gateway = gateway(Arc::new(provider.clone()), LlmConfig::default());

    let mut ctx = new_ctx(far_deadline());
    let scope = ctx.llm_call_scope(TurnStage::StoryGenerator);
    let completion = gateway.complete(scope, spec()).await.unwrap();
    assert_eq!(completion.text, "response text");
    let trace = ctx.trace().build(&StoryId::from("story-1"), &TurnId::from("turn-1"));
    let llm = trace.spans.iter().find(|s| s.kind == "aise.llm_call").expect("llm span");
    assert!(llm.payload.get("content").is_none());
    assert!(llm.payload.get("messages").is_none());
    assert!(llm.payload.get("response").is_none());
}

#[tokio::test]
async fn provider_timeout_precedes_turn_deadline_when_earlier() {
    let provider = MockProvider::new();
    let (tx, rx) = oneshot::channel();
    provider.set_block(rx);
    let config = LlmConfig {
        provider_timeout_ms: 50,
        ..LlmConfig::default()
    };
    let gateway = gateway(Arc::new(provider.clone()), config);

    let mut ctx = new_ctx(far_deadline());
    let scope = ctx.llm_call_scope(TurnStage::StoryGenerator);
    let error = gateway.complete(scope, spec()).await.unwrap_err();
    drop(tx);
    assert!(matches!(error, LlmError::ProviderTimeout));
}

#[tokio::test]
async fn turn_deadline_precedes_provider_timeout_when_earlier() {
    let provider = MockProvider::new();
    let (tx, rx) = oneshot::channel();
    provider.set_block(rx);
    let config = LlmConfig {
        provider_timeout_ms: 5_000,
        ..LlmConfig::default()
    };
    let gateway = gateway(Arc::new(provider.clone()), config);

    let mut ctx = new_ctx(Instant::now() + Duration::from_millis(80));
    let scope = ctx.llm_call_scope(TurnStage::StoryGenerator);
    let error = gateway.complete(scope, spec()).await.unwrap_err();
    drop(tx);
    assert!(matches!(error, LlmError::TurnDeadlineExceeded));
}

#[test]
fn embedding_returns_unsupported_typed_error() {
    let provider = MockProvider::new();
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut ctx = new_ctx(far_deadline());
        let _scope = ctx.llm_call_scope(TurnStage::StoryGenerator);
        let request = EmbeddingRequest {
            model: "m".into(),
            input: "text".into(),
        };
        let error = provider.embed(&request).await.unwrap_err();
        assert!(matches!(error, LlmError::EmbeddingUnsupported));
    });
}

#[test]
fn llm_error_kind_labels_are_stable() {
    assert_eq!(LlmError::Cancelled.kind(), "cancelled");
    assert_eq!(LlmError::TurnDeadlineExceeded.kind(), "turn_deadline_exceeded");
    assert_eq!(LlmError::ProviderTimeout.kind(), "provider_timeout");
    assert_eq!(LlmError::QueueTimeout.kind(), "queue_timeout");
    assert_eq!(LlmError::RateLimited.kind(), "rate_limited");
    assert_eq!(LlmError::TokenBudgetExceeded("x".into()).kind(), "token_budget_exceeded");
    assert_eq!(LlmError::ProviderRejected("x".into()).kind(), "provider_rejected");
    assert_eq!(LlmError::EmbeddingUnsupported.kind(), "embedding_unsupported");
}
