use aise::LlmConfig;
use aise::config::TurnConfig;
use aise::core::turn_budget::TurnBudget;
use aise::core::turn_context::TurnExecutionContext;
use aise::core::turn_contract::{
    IdempotencyKey, LlmCallPurpose, TurnCancellation, TurnControl, TurnIdentity, TurnRequest,
};
use aise::core::turn_pipeline::TurnStage;
use aise::core::turn_trace::TraceRecorder;
use aise::domain::ids::{StoryId, TurnId};
use aise::llm::LlmGateway;
use aise::llm::accounting::{FinishReason, LlmCompletion, LlmTokenUsage, UsageAccuracy};
use aise::llm::error::{LlmError, LlmProtocolErrorKind, LlmProviderError, LlmResponseLimit, LlmTransportErrorKind};
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
    empty: Arc<AtomicUsize>,
    reasoning_only: Arc<AtomicUsize>,
    protocol: Arc<AtomicUsize>,
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
            empty: Arc::new(AtomicUsize::new(0)),
            reasoning_only: Arc::new(AtomicUsize::new(0)),
            protocol: Arc::new(AtomicUsize::new(0)),
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

    fn set_empty(&self) {
        self.empty.store(1, Ordering::SeqCst);
    }

    fn set_reasoning_only(&self) {
        self.reasoning_only.store(1, Ordering::SeqCst);
    }

    fn set_protocol_error(&self) {
        self.protocol.store(1, Ordering::SeqCst);
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

    async fn enter(&self) -> Result<(), LlmProviderError> {
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

    async fn complete(&self, _req: &CompletionRequest) -> Result<LlmCompletion, LlmProviderError> {
        self.enter().await?;
        if self.protocol.load(Ordering::SeqCst) == 1 {
            return Err(LlmProviderError::Protocol {
                kind: LlmProtocolErrorKind::InvalidJson,
            });
        }
        if self.fail.load(Ordering::SeqCst) == 1 {
            return Err(LlmProviderError::Rejected {
                status: 400,
                code: None,
            });
        }
        let usage = self.usage.lock().unwrap().clone();
        let reasoning_only = self.reasoning_only.load(Ordering::SeqCst) == 1;
        let text = if self.empty.load(Ordering::SeqCst) == 1 || reasoning_only {
            String::new()
        } else {
            "response text".to_string()
        };
        let reasoning = if reasoning_only {
            Some("reasoning draft".to_string())
        } else {
            None
        };
        let finish_reason = if reasoning_only {
            Some(FinishReason::Length)
        } else {
            Some(FinishReason::Stop)
        };
        Ok(LlmCompletion {
            text,
            finish_reason,
            reasoning_content: reasoning,
            usage,
            charge: None,
        })
    }

    async fn complete_stream(
        &self,
        _req: &CompletionRequest,
        _on_delta: DeltaSink,
    ) -> Result<LlmCompletion, LlmProviderError> {
        self.enter().await?;
        let usage = self.usage.lock().unwrap().clone();
        let text = if self.empty.load(Ordering::SeqCst) == 1 {
            String::new()
        } else {
            "response text".to_string()
        };
        Ok(LlmCompletion {
            text,
            finish_reason: Some(FinishReason::Stop),
            reasoning_content: None,
            usage,
            charge: None,
        })
    }

    async fn embed(&self, _req: &EmbeddingRequest) -> Result<EmbeddingOutput, LlmProviderError> {
        Err(LlmProviderError::Protocol {
            kind: LlmProtocolErrorKind::Unsupported,
        })
    }
}

fn budget() -> TurnBudget {
    let config = TurnConfig {
        max_repair_rounds: 3,
        max_llm_calls: 16,
        max_input_tokens: 100_000,
        max_output_tokens: 2_048,
        max_total_tokens: 200_000,
        max_retrieved_items: 5,
        ..TurnConfig::default()
    };
    TurnBudget::from_config(&config, &aise::config::TurnContentLimitsConfig::default()).unwrap()
}

fn new_ctx(deadline: Instant) -> TurnExecutionContext {
    new_ctx_with_cancellation(deadline, TurnCancellation::new())
}

fn new_ctx_with_cancellation(deadline: Instant, cancellation: TurnCancellation) -> TurnExecutionContext {
    TurnExecutionContext::new(
        TurnIdentity::new(
            StoryId::try_new("story-1").unwrap(),
            TurnId::try_new("turn-1").unwrap(),
            IdempotencyKey::try_new("key-1".to_string()).unwrap(),
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
        purpose: LlmCallPurpose::StoryGeneration,
    }
}

fn gateway(provider: Arc<MockProvider>, config: LlmConfig) -> Arc<LlmGateway> {
    Arc::new(LlmGateway::new(provider, config).unwrap())
}

async fn call_complete(gateway: &LlmGateway, ctx: &mut TurnExecutionContext) -> Result<LlmCompletion, LlmError> {
    let spec = spec();
    let mut scope = ctx.llm_call_scope(TurnStage::StoryGenerator);
    let estimated = crate_estimate_input(&spec);
    let reservation = scope
        .reserve_llm(estimated, 64)
        .map_err(|error| LlmError::TokenBudgetExceeded(error.to_string()))?;
    gateway.complete(scope, spec, reservation).await
}

fn crate_estimate_input(spec: &CompletionSpec) -> u64 {
    crate_estimator::estimate(spec)
}

mod crate_estimator {
    use super::CompletionSpec;

    pub fn estimate(spec: &CompletionSpec) -> u64 {
        spec.messages
            .iter()
            .map(|m| m.content.chars().count() as u64 / 4)
            .sum::<u64>()
            .max(8)
    }
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
        call_complete(&g1, &mut ctx1).await
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(provider.calls(), 1);
    assert_eq!(provider.max_active(), 1);

    let g2 = gateway.clone();
    let second = tokio::spawn(async move {
        let mut ctx2 = new_ctx(far_deadline());
        call_complete(&g2, &mut ctx2).await
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
        call_complete(&g1, &mut ctx1).await
    });
    tokio::time::sleep(Duration::from_millis(30)).await;

    let g2 = gateway.clone();
    let second = tokio::spawn(async move {
        let mut ctx2 = new_ctx(far_deadline());
        call_complete(&g2, &mut ctx2).await
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
        call_complete(&g1, &mut ctx1).await
    });
    tokio::time::sleep(Duration::from_millis(30)).await;

    let g2 = gateway.clone();
    let second = tokio::spawn(async move {
        let mut ctx2 = new_ctx(Instant::now() + Duration::from_millis(150));
        call_complete(&g2, &mut ctx2).await
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
        call_complete(&task_gateway, &mut ctx).await
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
        let spec = spec();
        let mut scope = ctx.llm_call_scope(TurnStage::StoryGenerator);
        let estimated = crate_estimate_input(&spec);
        let reservation = scope.reserve_llm(estimated, 64).unwrap();
        task_gateway.complete_stream(scope, spec, reservation, sink).await
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

    let config = TurnConfig {
        max_repair_rounds: 0,
        max_llm_calls: 16,
        max_input_tokens: 16,
        max_output_tokens: 16,
        max_total_tokens: 24,
        max_retrieved_items: 5,
        ..TurnConfig::default()
    };
    let tight = TurnBudget::from_config(&config, &aise::config::TurnContentLimitsConfig::default()).unwrap();
    let mut ctx = TurnExecutionContext::new(
        TurnIdentity::new(
            StoryId::try_new("story-1").unwrap(),
            TurnId::try_new("turn-1").unwrap(),
            IdempotencyKey::try_new("key-1".to_string()).unwrap(),
            1,
        )
        .unwrap(),
        TurnRequest::try_new("hello".into()).unwrap(),
        tight,
        TurnControl::new(far_deadline(), TurnCancellation::new()),
        TraceRecorder::new(),
    )
    .unwrap();
    let error = call_complete(&gateway, &mut ctx).await.unwrap_err();
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
        reasoning_tokens: None,
        total_tokens: 150,
        accuracy: UsageAccuracy::Exact,
    });
    let gateway = gateway(Arc::new(provider.clone()), LlmConfig::default());

    let mut ctx = new_ctx(far_deadline());
    let completion = call_complete(&gateway, &mut ctx).await.unwrap();
    assert_eq!(completion.usage.as_ref().map(|u| u.input_tokens), Some(100));
    assert_eq!(ctx.budget().llm_calls(), 1);
}

#[tokio::test]
async fn finish_reason_is_persisted_in_call_ledger() {
    let provider = MockProvider::new();
    let gateway = gateway(Arc::new(provider.clone()), LlmConfig::default());

    let mut ctx = new_ctx(far_deadline());
    let completion = call_complete(&gateway, &mut ctx).await.unwrap();
    assert_eq!(completion.finish_reason, Some(FinishReason::Stop));
    assert_eq!(ctx.llm_calls().len(), 1);
    assert_eq!(
        ctx.llm_calls()[0].finish_reason,
        Some(FinishReason::Stop),
        "provider finish_reason must be persisted in the per-call ledger"
    );
}

#[tokio::test]
async fn missing_provider_usage_is_marked_estimated() {
    let provider = MockProvider::new();
    let gateway = gateway(Arc::new(provider.clone()), LlmConfig::default());

    let mut ctx = new_ctx(far_deadline());
    let completion = call_complete(&gateway, &mut ctx).await.unwrap();
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
        reasoning_tokens: Some(1_500),
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
    let completion = call_complete(&gateway, &mut ctx).await.unwrap();
    let charge = completion.charge.unwrap();
    assert_eq!(charge.amount_minor, 50);
}

#[tokio::test]
async fn llm_trace_closes_on_success() {
    let provider = MockProvider::new();
    let gateway = gateway(Arc::new(provider.clone()), LlmConfig::default());

    let mut ctx = new_ctx(far_deadline());
    call_complete(&gateway, &mut ctx).await.unwrap();
    let trace = ctx
        .trace()
        .build(&StoryId::try_new("story-1").unwrap(), &TurnId::try_new("turn-1").unwrap());
    let llm = trace.spans.iter().find(|s| s.kind == "aise.llm_call").expect("llm span");
    assert_eq!(llm.payload["status"], "succeeded");
    assert_eq!(llm.payload["error_kind"], serde_json::Value::Null);
}

#[tokio::test]
async fn llm_trace_closes_on_provider_error() {
    let provider = MockProvider::new();
    provider.set_fail();
    let gateway = gateway(Arc::new(provider.clone()), LlmConfig::default());

    let mut ctx = new_ctx(far_deadline());
    let error = call_complete(&gateway, &mut ctx).await.unwrap_err();
    assert!(matches!(error, LlmError::ProviderRejected { .. }));
    let trace = ctx
        .trace()
        .build(&StoryId::try_new("story-1").unwrap(), &TurnId::try_new("turn-1").unwrap());
    let llm = trace.spans.iter().find(|s| s.kind == "aise.llm_call").expect("llm span");
    assert_eq!(llm.payload["status"], "provider_rejected");
    assert_eq!(llm.payload["error_kind"], "provider_rejected");
}

#[tokio::test]
async fn empty_completion_returns_typed_protocol_error() {
    let provider = MockProvider::new();
    provider.set_empty();
    let gateway = gateway(Arc::new(provider.clone()), LlmConfig::default());

    let mut ctx = new_ctx(far_deadline());
    let error = call_complete(&gateway, &mut ctx).await.unwrap_err();
    assert!(
        matches!(
            &error,
            LlmError::Protocol {
                kind: LlmProtocolErrorKind::EmptyChoices
            }
        ),
        "unexpected error: {error}"
    );
    let trace = ctx
        .trace()
        .build(&StoryId::try_new("story-1").unwrap(), &TurnId::try_new("turn-1").unwrap());
    let llm = trace.spans.iter().find(|s| s.kind == "aise.llm_call").expect("llm span");
    assert_eq!(llm.payload["status"], "protocol_failed");
    assert_eq!(llm.payload["error_kind"], "protocol");
}

#[tokio::test]
async fn reasoning_only_completion_is_still_treated_as_empty() {
    let provider = MockProvider::new();
    provider.set_reasoning_only();
    let gateway = gateway(Arc::new(provider.clone()), LlmConfig::default());

    let mut ctx = new_ctx(far_deadline());
    let error = call_complete(&gateway, &mut ctx).await.unwrap_err();
    assert!(
        matches!(
            &error,
            LlmError::Protocol {
                kind: LlmProtocolErrorKind::EmptyChoices
            }
        ),
        "reasoning-only content must not be used as output: {error}"
    );
}

#[tokio::test]
async fn reasoning_tokens_are_recorded_in_trace() {
    let provider = MockProvider::new();
    provider.set_usage(LlmTokenUsage {
        input_tokens: 100,
        cached_input_tokens: None,
        output_tokens: 512,
        reasoning_tokens: Some(512),
        total_tokens: 612,
        accuracy: UsageAccuracy::Exact,
    });
    let gateway = gateway(Arc::new(provider.clone()), LlmConfig::default());

    let mut ctx = new_ctx(far_deadline());
    call_complete(&gateway, &mut ctx).await.unwrap();
    let trace = ctx
        .trace()
        .build(&StoryId::try_new("story-1").unwrap(), &TurnId::try_new("turn-1").unwrap());
    let llm = trace.spans.iter().find(|s| s.kind == "aise.llm_call").expect("llm span");
    assert_eq!(llm.payload["reasoning_tokens"], 512);
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
    let error = call_complete(&gateway, &mut ctx).await.unwrap_err();
    assert!(matches!(error, LlmError::ProviderTimeout));
    let trace = ctx
        .trace()
        .build(&StoryId::try_new("story-1").unwrap(), &TurnId::try_new("turn-1").unwrap());
    let llm = trace.spans.iter().find(|s| s.kind == "aise.llm_call").expect("llm span");
    assert_eq!(llm.payload["status"], "provider_timeout");
    assert_eq!(llm.payload["error_kind"], "provider_timeout");
    let _ = tx.send(());
}

#[tokio::test]
async fn default_trace_does_not_store_prompt_or_response_text() {
    let provider = MockProvider::new();
    let gateway = gateway(Arc::new(provider.clone()), LlmConfig::default());

    let mut ctx = new_ctx(far_deadline());
    let completion = call_complete(&gateway, &mut ctx).await.unwrap();
    assert_eq!(completion.text, "response text");
    let trace = ctx
        .trace()
        .build(&StoryId::try_new("story-1").unwrap(), &TurnId::try_new("turn-1").unwrap());
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
    let error = call_complete(&gateway, &mut ctx).await.unwrap_err();
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
    let error = call_complete(&gateway, &mut ctx).await.unwrap_err();
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
            inputs: vec!["text".into()],
        };
        let error = provider.embed(&request).await.unwrap_err();
        assert!(matches!(
            error,
            LlmProviderError::Protocol {
                kind: LlmProtocolErrorKind::Unsupported
            }
        ));
    });
}

#[test]
fn llm_error_kind_labels_are_stable() {
    assert_eq!(LlmError::Cancelled.kind(), "cancelled");
    assert_eq!(LlmError::TurnDeadlineExceeded.kind(), "turn_deadline_exceeded");
    assert_eq!(LlmError::ProviderTimeout.kind(), "provider_timeout");
    assert_eq!(LlmError::QueueTimeout.kind(), "queue_timeout");
    assert_eq!(LlmError::RateLimited { retry_after_ms: None }.kind(), "rate_limited");
    assert_eq!(LlmError::TokenBudgetExceeded("x".into()).kind(), "token_budget_exceeded");
    assert_eq!(LlmError::ProviderRejected { status: 400 }.kind(), "provider_rejected");
    assert_eq!(LlmError::EmbeddingUnsupported.kind(), "embedding_unsupported");
    assert_eq!(
        LlmError::Transport {
            kind: LlmTransportErrorKind::Connect
        }
        .kind(),
        "transport"
    );
    assert_eq!(
        LlmError::Protocol {
            kind: LlmProtocolErrorKind::InvalidJson
        }
        .kind(),
        "protocol"
    );
    assert_eq!(
        LlmError::ResponseLimitExceeded {
            limit: LlmResponseLimit::Content
        }
        .kind(),
        "response_limit_exceeded"
    );
}

#[tokio::test]
async fn llm_trace_closes_on_pre_cancel() {
    let provider = MockProvider::new();
    let gateway = gateway(Arc::new(provider.clone()), LlmConfig::default());

    let cancellation = TurnCancellation::new();
    cancellation.cancel();
    let mut ctx = new_ctx_with_cancellation(far_deadline(), cancellation);
    let error = call_complete(&gateway, &mut ctx).await.unwrap_err();
    assert!(matches!(error, LlmError::Cancelled));
    assert_eq!(provider.calls(), 0, "pre-cancel never reaches the provider");
    let trace = ctx
        .trace()
        .build(&StoryId::try_new("story-1").unwrap(), &TurnId::try_new("turn-1").unwrap());
    let llm = trace.spans.iter().find(|s| s.kind == "aise.llm_call").expect("llm span");
    assert_eq!(llm.payload["status"], "cancelled");
    assert_eq!(llm.payload["error_kind"], "cancelled");
}

#[tokio::test]
async fn llm_trace_closes_on_turn_deadline_before_queue() {
    let provider = MockProvider::new();
    let gateway = gateway(Arc::new(provider.clone()), LlmConfig::default());

    let mut ctx = new_ctx(Instant::now() - Duration::from_secs(1));
    let error = call_complete(&gateway, &mut ctx).await.unwrap_err();
    assert!(matches!(error, LlmError::TurnDeadlineExceeded));
    assert_eq!(provider.calls(), 0, "deadline-before-queue never reaches the provider");
    let trace = ctx
        .trace()
        .build(&StoryId::try_new("story-1").unwrap(), &TurnId::try_new("turn-1").unwrap());
    let llm = trace.spans.iter().find(|s| s.kind == "aise.llm_call").expect("llm span");
    assert_eq!(llm.payload["status"], "turn_deadline_exceeded");
    assert_eq!(llm.payload["error_kind"], "turn_deadline_exceeded");
}

#[tokio::test]
async fn llm_trace_closes_on_queue_timeout() {
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
        call_complete(&g1, &mut ctx1).await
    });
    tokio::time::sleep(Duration::from_millis(30)).await;

    let mut ctx2 = new_ctx(far_deadline());
    let error = call_complete(&gateway, &mut ctx2).await.unwrap_err();
    assert!(matches!(error, LlmError::QueueTimeout));
    let trace = ctx2
        .trace()
        .build(&StoryId::try_new("story-1").unwrap(), &TurnId::try_new("turn-1").unwrap());
    let llm = trace.spans.iter().find(|s| s.kind == "aise.llm_call").expect("llm span");
    assert_eq!(llm.payload["status"], "queue_timeout");
    assert_eq!(llm.payload["error_kind"], "queue_timeout");

    tx.send(()).unwrap();
    let _ = first.await.unwrap().unwrap();
}

#[tokio::test]
async fn pending_reservation_reduces_available_budget() {
    let config = TurnConfig {
        max_repair_rounds: 3,
        max_llm_calls: 8,
        max_input_tokens: 8_192,
        max_output_tokens: 2_048,
        max_total_tokens: 10_240,
        max_retrieved_items: 5,
        ..TurnConfig::default()
    };
    let mut budget = TurnBudget::from_config(&config, &aise::config::TurnContentLimitsConfig::default()).unwrap();
    let before = budget.remaining_output_tokens();

    let first = budget.reserve_llm(100, 200).unwrap();
    assert_eq!(
        budget.remaining_output_tokens(),
        before - 200,
        "pending reservation counts against budget"
    );
    let second = budget.reserve_llm(100, 400).unwrap();
    assert_eq!(
        budget.remaining_output_tokens(),
        before - 600,
        "multiple pending reservations stack"
    );

    budget.release_llm(first);
    assert_eq!(budget.remaining_output_tokens(), before - 400);
    budget.release_llm(second);
    assert_eq!(budget.remaining_output_tokens(), before, "release restores available budget");
}

#[tokio::test]
async fn settlement_overflow_marks_trace_budget_exceeded() {
    let provider = MockProvider::new();
    provider.set_usage(LlmTokenUsage {
        input_tokens: 1_000,
        cached_input_tokens: None,
        output_tokens: 1_000,
        reasoning_tokens: None,
        total_tokens: 2_000,
        accuracy: UsageAccuracy::Exact,
    });
    let gateway = gateway(Arc::new(provider.clone()), LlmConfig::default());

    let config = TurnConfig {
        max_repair_rounds: 0,
        max_llm_calls: 16,
        max_input_tokens: 1_024,
        max_output_tokens: 1_024,
        max_total_tokens: 1_024,
        max_retrieved_items: 5,
        ..TurnConfig::default()
    };
    let tight = TurnBudget::from_config(&config, &aise::config::TurnContentLimitsConfig::default()).unwrap();
    let mut ctx = TurnExecutionContext::new(
        TurnIdentity::new(
            StoryId::try_new("story-1").unwrap(),
            TurnId::try_new("turn-1").unwrap(),
            IdempotencyKey::try_new("key-1".to_string()).unwrap(),
            1,
        )
        .unwrap(),
        TurnRequest::try_new("hello".into()).unwrap(),
        tight,
        TurnControl::new(far_deadline(), TurnCancellation::new()),
        TraceRecorder::new(),
    )
    .unwrap();
    let error = call_complete(&gateway, &mut ctx).await.unwrap_err();
    assert!(matches!(error, LlmError::TokenBudgetExceeded(_)));
    let trace = ctx
        .trace()
        .build(&StoryId::try_new("story-1").unwrap(), &TurnId::try_new("turn-1").unwrap());
    let llm = trace.spans.iter().find(|s| s.kind == "aise.llm_call").expect("llm span");
    assert_eq!(llm.payload["status"], "token_budget_exceeded");
    assert_ne!(llm.payload["status"], "succeeded", "overflow never records success");
}

#[tokio::test]
async fn metadata_only_parse_error_contains_no_model_output() {
    let provider = MockProvider::new();
    provider.set_protocol_error();
    let gateway = gateway(Arc::new(provider.clone()), LlmConfig::default());

    let mut ctx = new_ctx(far_deadline());
    let error = call_complete(&gateway, &mut ctx).await.unwrap_err();
    assert!(matches!(
        error,
        LlmError::Protocol {
            kind: LlmProtocolErrorKind::InvalidJson
        }
    ));

    let trace = ctx
        .trace()
        .build(&StoryId::try_new("story-1").unwrap(), &TurnId::try_new("turn-1").unwrap());
    let llm = trace.spans.iter().find(|s| s.kind == "aise.llm_call").expect("llm span");
    assert_eq!(llm.payload["status"], "protocol_failed");
    assert_eq!(llm.payload["error_kind"], "protocol");
    assert!(
        llm.payload.get("content").is_none_or(|c| c.is_null()),
        "MetadataOnly trace carries no response content"
    );
    let serialized = serde_json::to_string(&llm.payload).expect("serialize payload");
    assert!(
        !serialized.contains("response text"),
        "model output must not leak into the trace under MetadataOnly"
    );
}
