use aise::AiseConfig;
use aise::AiseEngine;
use aise::TurnEvent;
use aise::TurnEventSink;
use aise::character::CharacterThinkPipeline;
use aise::context::{BaselineContextBuilder, ContextRetrievalPipeline};
use aise::core::turn_contract::{
    CommittedTurnResult, IdempotencyKey, LlmCallPurpose, LlmUsageAggregate, TurnCancellation,
};
use aise::core::turn_data::SnapshotLimits;
use aise::core::turn_pipeline::TurnStage;
use aise::domain::ids::StoryRevision;
use aise::engine::{SystemClock, UuidIdGenerator};
use aise::llm::LlmGateway;
use aise::llm::accounting::{FinishReason, LlmCompletion, LlmTokenUsage, UsageAccuracy};
use aise::llm::error::{LlmProtocolErrorKind, LlmProviderError};
use aise::llm::message::{CompletionRequest, EmbeddingOutput, EmbeddingRequest};
use aise::llm::provider::{DeltaSink, LlmProvider};
use aise::persistence::{SqliteStore, TurnCommitter};
use aise::planning::WriterPlanner;
use aise::runtime::{StoryTurnCoordinator, TurnInitializer, TurnPipelineSet, TurnRuntime};
use aise::story::{StoryGenerator, StoryRepairer};
use aise::validation::ValidationPipeline;
use aise_server::api::sse::{ClientDisconnectGuard, SSE_CHANNEL_CAPACITY, SseSink, sse_merged_stream};
use aise_server::session::SessionRegistry;
use aise_server::tasks::{TurnTaskSupervisor, TurnTaskSupervisorConfig};
use aise_server::{AppState, ServerConfig, router};
use futures::StreamExt;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, oneshot};

#[tokio::test]
async fn bounded_sse_channel_applies_backpressure() {
    let (progress_tx, _progress_rx) = mpsc::channel::<axum::response::sse::Event>(SSE_CHANNEL_CAPACITY);
    let (terminal_tx, _terminal_rx) = mpsc::channel::<axum::response::sse::Event>(1);
    let sink = SseSink::new(progress_tx, terminal_tx, false);
    let mut emitted = 0usize;
    loop {
        let delivered = sink.emit(TurnEvent::StageStarted {
            turn_id: aise::domain::ids::TurnId::try_new("turn-1").unwrap(),
            stage: TurnStage::TurnInitializer,
        });
        emitted += 1;
        if delivered.is_err() {
            break;
        }
        assert!(
            emitted < SSE_CHANNEL_CAPACITY + 8,
            "channel must stay bounded near its configured capacity"
        );
    }
    assert_eq!(
        emitted,
        SSE_CHANNEL_CAPACITY + 1,
        "backpressure engages exactly at the configured capacity"
    );
    assert_eq!(sink.dropped_events(), 0, "no events dropped, only backpressure");
}

#[test]
fn client_disconnect_guard_cancels_turn() {
    let cancellation = TurnCancellation::new();
    {
        let _guard = ClientDisconnectGuard::new(cancellation.clone());
        assert!(!cancellation.is_cancelled());
    }
    assert!(cancellation.is_cancelled(), "dropping the sse stream cancels the turn");
}

#[tokio::test]
async fn terminal_event_survives_saturated_progress_lane() {
    let (progress_tx, _progress_rx) = mpsc::channel::<axum::response::sse::Event>(SSE_CHANNEL_CAPACITY);
    let (terminal_tx, mut terminal_rx) = mpsc::channel::<axum::response::sse::Event>(1);
    let sink = SseSink::new(progress_tx, terminal_tx, false);
    let turn_id = aise::domain::ids::TurnId::try_new("turn-1").unwrap();
    loop {
        if sink
            .emit(TurnEvent::StageStarted {
                turn_id: turn_id.clone(),
                stage: TurnStage::TurnInitializer,
            })
            .is_err()
        {
            break;
        }
    }
    let result = CommittedTurnResult {
        turn_id,
        story_revision: StoryRevision::new(1),
        story_text: "terminal survives".into(),
        llm_usage: LlmUsageAggregate::default(),
        llm_calls: Vec::new(),
    };
    assert!(
        sink.emit(TurnEvent::Committed {
            result,
            replayed: false
        })
        .is_ok(),
        "terminal event must be accepted even when the progress lane is saturated"
    );
    let event = terminal_rx.try_recv().expect("terminal lane delivers the committed event");
    let _ = event;
    assert_eq!(sink.dropped_events(), 0, "terminal event must not be dropped");
}

#[tokio::test]
async fn http_preflight_error_does_not_open_sse() {
    let db = temp_db_path("preflight");
    let calls = Arc::new(AtomicUsize::new(0));
    let provider: Arc<dyn LlmProvider> = Arc::new(CountingProvider { calls: calls.clone() });
    let engine = build_engine(&db, provider).await;

    let story_id = aise::domain::StoryId::try_new("story-preflight-1").unwrap();
    let spec = aise::domain::StoryCreateSpec {
        story_id: story_id.clone(),
        story_instructions: String::new(),
        story_config: aise::domain::StoryConfig::default(),
        player_character_id: None,
        initial_world: None,
        current_scene: aise::domain::CurrentScene { text: String::new() },
        story_summary: aise::domain::StorySummary { text: String::new() },
        active_constraints: Vec::new(),
        created_at_ms: 1000,
    };
    engine.store().create_story(&spec).await.expect("create story");

    let registry = SessionRegistry::new(8);
    let session = registry.create("test".into(), story_id).await.expect("session");
    let session_id = session.id.as_str().to_string();
    let tasks = TurnTaskSupervisor::new(TurnTaskSupervisorConfig::default()).unwrap();
    let config = ServerConfig::default();
    let state = Arc::new(AppState::new(engine, registry, tasks.clone(), config.clone()));
    let app = router(state, &config);

    let request = axum::http::Request::builder()
        .method("POST")
        .uri(format!("/api/sessions/{session_id}/turns"))
        .header("Content-Type", "application/json")
        .header("Idempotency-Key", "key-preflight")
        .body(axum::body::Body::from(
            serde_json::json!({ "player_input": "", "include_trace": false }).to_string(),
        ))
        .unwrap();
    let response = tokio::time::timeout(Duration::from_secs(5), tower::ServiceExt::oneshot(app, request))
        .await
        .expect("router call within timeout")
        .expect("router call");
    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
    let body = response.into_body();
    let bytes = axum::body::to_bytes(body, 1024 * 1024).await.expect("read error body");
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("preflight failure returns JSON, not SSE");
    assert!(json["error"].is_string(), "error payload present: {json}");
    assert_eq!(tasks.active_turns().await, 0, "no turn task is spawned on preflight failure");

    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn sse_stream_ends_when_sender_drops() {
    let (progress_tx, progress_rx) = mpsc::channel::<axum::response::sse::Event>(4);
    let (terminal_tx, terminal_rx) = mpsc::channel::<axum::response::sse::Event>(1);
    let cancellation = TurnCancellation::new();
    let stream = sse_merged_stream(progress_rx, terminal_rx, ClientDisconnectGuard::new(cancellation.clone()));
    futures::pin_mut!(stream);
    let sink = SseSink::new(progress_tx, terminal_tx, false);
    sink.emit(TurnEvent::StageStarted {
        turn_id: aise::domain::ids::TurnId::try_new("turn-1").unwrap(),
        stage: TurnStage::TurnInitializer,
    })
    .unwrap();
    let item = stream.next().await;
    assert!(item.is_some());

    drop(sink);
    let end = stream.next().await;
    assert!(end.is_none(), "stream completes naturally when the turn sender drops");
    assert!(cancellation.is_cancelled());
}

struct BlockingProvider {
    block: tokio::sync::Mutex<Option<oneshot::Receiver<()>>>,
    entered: Arc<tokio::sync::Notify>,
}

#[async_trait::async_trait]
impl LlmProvider for BlockingProvider {
    fn provider_name(&self) -> &'static str {
        "blocking"
    }

    async fn complete(&self, _req: &CompletionRequest) -> Result<LlmCompletion, LlmProviderError> {
        self.entered.notify_one();
        let rx = self.block.lock().await.take().expect("single blocking call");
        let _ = rx.await;
        Ok(LlmCompletion {
            text: "story".into(),
            finish_reason: Some(FinishReason::Stop),
            reasoning_content: None,
            usage: None,
            charge: None,
        })
    }

    async fn complete_stream(
        &self,
        _req: &CompletionRequest,
        _on_delta: DeltaSink,
    ) -> Result<LlmCompletion, LlmProviderError> {
        Err(LlmProviderError::Protocol {
            kind: LlmProtocolErrorKind::Unsupported,
        })
    }

    async fn embed(&self, _req: &EmbeddingRequest) -> Result<EmbeddingOutput, LlmProviderError> {
        Err(LlmProviderError::Protocol {
            kind: LlmProtocolErrorKind::Unsupported,
        })
    }
}

struct CountingProvider {
    calls: Arc<AtomicUsize>,
}

fn story_proposal_json(story: &str) -> String {
    format!(
        r#"{{"story_text":"{story}","events":[{{"kind":"action","summary":"{story}"}}],"character_changes":[],"world_change":{{"add_facts":[]}},"memory_changes":[],"summary_change":null}}"#
    )
}

fn counting_completion_text(purpose: LlmCallPurpose) -> String {
    match purpose {
        LlmCallPurpose::WriterPlan => {
            r#"{"retrieval_requests":[],"character_requests":[],"story_goal":{"summary":""}}"#.to_string()
        }
        LlmCallPurpose::StoryGeneration | LlmCallPurpose::StoryRepair => story_proposal_json("Hello World"),
        _ => "Hello World".to_string(),
    }
}

#[async_trait::async_trait]
impl LlmProvider for CountingProvider {
    fn provider_name(&self) -> &'static str {
        "counting"
    }

    async fn complete(&self, req: &CompletionRequest) -> Result<LlmCompletion, LlmProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(LlmCompletion {
            text: counting_completion_text(req.purpose),
            finish_reason: Some(FinishReason::Stop),
            reasoning_content: None,
            usage: Some(LlmTokenUsage {
                input_tokens: 10,
                cached_input_tokens: None,
                output_tokens: 20,
                reasoning_tokens: None,
                total_tokens: 30,
                accuracy: UsageAccuracy::Exact,
            }),
            charge: None,
        })
    }

    async fn complete_stream(
        &self,
        _req: &CompletionRequest,
        _on_delta: DeltaSink,
    ) -> Result<LlmCompletion, LlmProviderError> {
        Err(LlmProviderError::Protocol {
            kind: LlmProtocolErrorKind::Unsupported,
        })
    }

    async fn embed(&self, _req: &EmbeddingRequest) -> Result<EmbeddingOutput, LlmProviderError> {
        Err(LlmProviderError::Protocol {
            kind: LlmProtocolErrorKind::Unsupported,
        })
    }
}

async fn build_engine(db_url: &str, provider: Arc<dyn LlmProvider>) -> Arc<AiseEngine> {
    let store = SqliteStore::connect(db_url).await.expect("connect store");
    let config = AiseConfig::default();
    let gateway = Arc::new(LlmGateway::new(provider, config.llm.clone()).expect("gateway"));
    let coordinator = StoryTurnCoordinator::new(&config.coordinator);
    let store_arc: Arc<dyn aise::persistence::Store> = store;
    let pipeline_set = TurnPipelineSet::builder()
        .initializer(Box::<TurnInitializer>::default())
        .baseline_builder(Box::new(BaselineContextBuilder::new(store_arc.clone(), config.content.clone())))
        .writer_planner(Box::new(WriterPlanner::new(gateway.clone())))
        .retrieval(Box::new(ContextRetrievalPipeline))
        .character_think(Box::new(CharacterThinkPipeline::new(gateway.clone())))
        .story_generator(Box::new(StoryGenerator::new(gateway.clone())))
        .validation(Box::new(ValidationPipeline::default()))
        .story_repairer(Box::new(StoryRepairer::new(gateway.clone())))
        .committer(Box::new(TurnCommitter::new(store_arc.clone())))
        .build()
        .expect("pipeline set");
    let runtime = TurnRuntime::new(pipeline_set);
    Arc::new(AiseEngine::new(
        runtime,
        store_arc,
        coordinator,
        config,
        Arc::new(UuidIdGenerator),
        Arc::new(SystemClock),
    ))
}

fn snapshot_limits() -> SnapshotLimits {
    SnapshotLimits::from_config(&aise::config::TurnContentLimitsConfig::default())
}

fn temp_db_path(label: &str) -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir()
        .join(format!("aise_server_{label}_{now}.db"))
        .to_string_lossy()
        .into_owned()
}

async fn wait_until_async<F, Fut>(mut predicate: F, timeout: Duration)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    tokio::time::timeout(timeout, async {
        while !predicate().await {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("condition not reached within timeout");
}

#[tokio::test]
async fn client_disconnect_cancels_uncommitted_turn() {
    let db = temp_db_path("sse_disconnect");
    let (block_tx, block_rx) = oneshot::channel();
    let entered = Arc::new(tokio::sync::Notify::new());
    let provider: Arc<dyn LlmProvider> = Arc::new(BlockingProvider {
        block: tokio::sync::Mutex::new(Some(block_rx)),
        entered: entered.clone(),
    });
    let engine = build_engine(&db, provider).await;
    let store = engine.store().clone();

    let story_id = aise::domain::StoryId::try_new("story-sse-1").unwrap();
    let spec = aise::domain::StoryCreateSpec {
        story_id: story_id.clone(),
        story_instructions: String::new(),
        story_config: aise::domain::StoryConfig::default(),
        player_character_id: None,
        initial_world: None,
        current_scene: aise::domain::CurrentScene { text: String::new() },
        story_summary: aise::domain::StorySummary { text: String::new() },
        active_constraints: Vec::new(),
        created_at_ms: 1000,
    };
    store.create_story(&spec).await.expect("create story");

    let registry = SessionRegistry::new(8);
    let session = registry.create("test".into(), story_id.clone()).await.expect("session");
    let session_id = session.id.as_str().to_string();
    let tasks = TurnTaskSupervisor::new(TurnTaskSupervisorConfig::default()).unwrap();
    let config = ServerConfig::default();
    let state = Arc::new(AppState::new(engine, registry, tasks.clone(), config.clone()));
    let app = router(state, &config);

    let request = axum::http::Request::builder()
        .method("POST")
        .uri(format!("/api/sessions/{session_id}/turns"))
        .header("Content-Type", "application/json")
        .header("Idempotency-Key", "key-1")
        .body(axum::body::Body::from(
            serde_json::json!({ "player_input": "开始吧", "include_trace": false }).to_string(),
        ))
        .unwrap();
    let response = tokio::time::timeout(Duration::from_secs(5), tower::ServiceExt::oneshot(app, request))
        .await
        .expect("router call within timeout")
        .expect("router call");
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    tokio::time::timeout(Duration::from_secs(5), entered.notified())
        .await
        .expect("turn task reached the LLM provider");

    let body = response.into_body();
    let tasks_for_wait = tasks.clone();
    wait_until_async(
        move || {
            let manager = tasks_for_wait.clone();
            async move { manager.active_turns().await == 1 }
        },
        Duration::from_secs(5),
    )
    .await;
    assert_eq!(tasks.active_turns().await, 1, "turn is mid-flight, blocked on the provider");

    drop(body);

    let tasks_for_wait = tasks.clone();
    wait_until_async(
        move || {
            let manager = tasks_for_wait.clone();
            async move { manager.active_turns().await == 0 }
        },
        Duration::from_secs(5),
    )
    .await;
    assert_eq!(
        tasks.active_turns().await,
        0,
        "turn task cancelled and finished after disconnect"
    );

    let key = IdempotencyKey::try_new("key-1".to_string()).unwrap();
    assert!(
        store
            .find_committed_turn(&story_id, &key)
            .await
            .expect("lookup committed turn")
            .is_none(),
        "cancelled turn must not commit"
    );
    let snapshot = store
        .load_story_snapshot(&story_id, snapshot_limits())
        .await
        .expect("load snapshot");
    assert_eq!(snapshot.recent_turns().len(), 0, "no turn record persisted for cancelled turn");

    let _ = block_tx.send(());
    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn get_turn_result_recovers_after_sse_disconnect() {
    let db = temp_db_path("sse_recover");
    let calls = Arc::new(AtomicUsize::new(0));
    let provider: Arc<dyn LlmProvider> = Arc::new(CountingProvider { calls: calls.clone() });
    let engine = build_engine(&db, provider).await;

    let story_id = aise::domain::StoryId::try_new("story-recover-1").unwrap();
    let spec = aise::domain::StoryCreateSpec {
        story_id: story_id.clone(),
        story_instructions: String::new(),
        story_config: aise::domain::StoryConfig::default(),
        player_character_id: None,
        initial_world: None,
        current_scene: aise::domain::CurrentScene { text: String::new() },
        story_summary: aise::domain::StorySummary { text: String::new() },
        active_constraints: Vec::new(),
        created_at_ms: 1000,
    };
    engine.store().create_story(&spec).await.expect("create story");

    let registry = SessionRegistry::new(8);
    let session = registry.create("test".into(), story_id.clone()).await.expect("session");
    let session_id = session.id.as_str().to_string();
    let tasks = TurnTaskSupervisor::new(TurnTaskSupervisorConfig::default()).unwrap();
    let config = ServerConfig::default();
    let state = Arc::new(AppState::new(engine.clone(), registry, tasks.clone(), config.clone()));
    let app = router(state, &config);

    let request = axum::http::Request::builder()
        .method("POST")
        .uri(format!("/api/sessions/{session_id}/turns"))
        .header("Content-Type", "application/json")
        .header("Idempotency-Key", "key-recover")
        .body(axum::body::Body::from(
            serde_json::json!({ "player_input": "开始吧", "include_trace": false }).to_string(),
        ))
        .unwrap();
    let response = tokio::time::timeout(Duration::from_secs(10), tower::ServiceExt::oneshot(app.clone(), request))
        .await
        .expect("router call within timeout")
        .expect("router call");
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let body = response.into_body();
    let bytes = axum::body::to_bytes(body, 16 * 1024 * 1024).await.expect("read sse body");
    let text = String::from_utf8_lossy(&bytes).to_string();
    assert!(
        text.contains("event: committed"),
        "committed terminal delivered via SSE; body: {}",
        text
    );

    let initial_calls = calls.load(Ordering::SeqCst);
    assert_eq!(initial_calls, 2, "turn used writer planner + story generation calls");

    let recover = axum::http::Request::builder()
        .method("GET")
        .uri(format!("/api/stories/{}/turn-results/key-recover", story_id.as_str()))
        .body(axum::body::Body::empty())
        .unwrap();
    let response = tokio::time::timeout(Duration::from_secs(5), tower::ServiceExt::oneshot(app, recover))
        .await
        .expect("router call within timeout")
        .expect("router call");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = response.into_body();
    let bytes = axum::body::to_bytes(body, 1024 * 1024).await.expect("read recovery body");
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("parse recovery json");
    assert_eq!(json["story_text"], "Hello World", "committed result recovered");
    assert_eq!(calls.load(Ordering::SeqCst), initial_calls, "recovery performs no llm call");

    let _ = std::fs::remove_file(&db);
}
