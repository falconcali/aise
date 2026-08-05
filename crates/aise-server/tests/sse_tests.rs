use aise::AiseConfig;
use aise::AiseEngine;
use aise::TurnEvent;
use aise::TurnEventSink;
use aise::character::CharacterThinkPipeline;
use aise::context::{BaselineContextBuilder, ContextRetrievalPipeline};
use aise::core::turn_contract::TurnCancellation;
use aise::core::turn_data::SnapshotLimits;
use aise::core::turn_pipeline::TurnStage;
use aise::llm::LlmGateway;
use aise::llm::accounting::{FinishReason, LlmCompletion};
use aise::llm::error::LlmError;
use aise::llm::message::{CompletionRequest, EmbeddingOutput, EmbeddingRequest};
use aise::llm::provider::{DeltaSink, LlmProvider};
use aise::persistence::{SqliteStore, TurnCommitter};
use aise::planning::WriterPlanner;
use aise::runtime::{StoryTurnCoordinator, TurnInitializer, TurnPipelineSet, TurnRuntime};
use aise::story::{StoryGenerator, StoryRepairer};
use aise::validation::ValidationPipeline;
use aise_server::api::sse::{ClientDisconnectGuard, SSE_CHANNEL_CAPACITY, SseSink, sse_stream};
use aise_server::session::SessionRegistry;
use aise_server::tasks::TurnTaskManager;
use aise_server::{AppState, ServerConfig, router};
use futures::StreamExt;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::oneshot;

#[test]
fn bounded_sse_channel_applies_backpressure() {
    let (tx, mut rx) = futures::channel::mpsc::channel::<axum::response::sse::Event>(SSE_CHANNEL_CAPACITY);
    let sink = SseSink::new(tx, false);
    let mut emitted = 0usize;
    loop {
        sink.emit(TurnEvent::StageStarted(TurnStage::TurnInitializer));
        emitted += 1;
        if sink.dropped_events() > 0 {
            break;
        }
        assert!(
            emitted < SSE_CHANNEL_CAPACITY + 8,
            "channel must stay bounded near its configured capacity"
        );
    }
    assert_eq!(sink.dropped_events(), 1, "exactly one overflow event dropped");
    assert!(
        (SSE_CHANNEL_CAPACITY..=SSE_CHANNEL_CAPACITY + 2).contains(&emitted),
        "backpressure engages at the configured capacity, emitted={emitted}"
    );

    let _delivered = rx.try_recv().expect("buffered event");
    sink.emit(TurnEvent::StageStarted(TurnStage::BaselineBuilder));
    assert_eq!(sink.dropped_events(), 1, "freed slot accepts a new event");
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
async fn sse_stream_ends_when_sender_drops() {
    let (tx, rx) = futures::channel::mpsc::channel::<axum::response::sse::Event>(4);
    let cancellation = TurnCancellation::new();
    let stream = sse_stream(rx, ClientDisconnectGuard::new(cancellation.clone()));
    futures::pin_mut!(stream);
    let sink = SseSink::new(tx, false);
    sink.emit(TurnEvent::StageStarted(TurnStage::TurnInitializer));
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

    async fn complete(&self, _req: &CompletionRequest) -> Result<LlmCompletion, LlmError> {
        self.entered.notify_one();
        let rx = self.block.lock().await.take().expect("single blocking call");
        let _ = rx.await;
        Ok(LlmCompletion {
            text: "story".into(),
            finish_reason: Some(FinishReason::Stop),
            usage: None,
            charge: None,
        })
    }

    async fn complete_stream(&self, _req: &CompletionRequest, _on_delta: DeltaSink) -> Result<LlmCompletion, LlmError> {
        Err(LlmError::Protocol("not used".into()))
    }

    async fn embed(&self, _req: &EmbeddingRequest) -> Result<EmbeddingOutput, LlmError> {
        Err(LlmError::EmbeddingUnsupported)
    }
}

async fn build_engine(db_url: &str, provider: Arc<dyn LlmProvider>) -> Arc<AiseEngine> {
    let store = SqliteStore::connect(db_url).await.expect("connect store");
    let config = AiseConfig::default();
    let gateway = Arc::new(LlmGateway::new(provider, config.llm.clone()).expect("gateway"));
    let coordinator = StoryTurnCoordinator::new(&config.coordinator);
    let pipeline_set = TurnPipelineSet::builder()
        .initializer(Box::<TurnInitializer>::default())
        .baseline_builder(Box::new(BaselineContextBuilder::new(store.clone())))
        .writer_planner(Box::new(WriterPlanner))
        .retrieval(Box::new(ContextRetrievalPipeline))
        .character_think(Box::new(CharacterThinkPipeline))
        .story_generator(Box::new(StoryGenerator::new(gateway.clone())))
        .validation(Box::new(ValidationPipeline::default()))
        .story_repairer(Box::new(StoryRepairer::new(gateway.clone())))
        .committer(Box::new(TurnCommitter::new(store.clone())))
        .build()
        .expect("pipeline set");
    let runtime = TurnRuntime::new(pipeline_set);
    Arc::new(AiseEngine::new(runtime, store, coordinator, config))
}

fn snapshot_limits() -> SnapshotLimits {
    SnapshotLimits {
        max_recent_turns: 20,
        max_memories: 20,
    }
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

    let registry = SessionRegistry::new(8);
    let session = registry.create("test".into()).await.expect("session");
    let session_id = session.id.as_str().to_string();
    let story_id = session.story_id.clone();
    let tasks = Arc::new(TurnTaskManager::new(8).unwrap());
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

    let key = aise::core::turn_contract::IdempotencyKey::try_new("key-1".to_string()).unwrap();
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
    assert_eq!(
        snapshot.expect("story row created by engine").recent_turns().len(),
        0,
        "no turn record persisted for cancelled turn"
    );

    let _ = block_tx.send(());
    let _ = std::fs::remove_file(&db);
}
