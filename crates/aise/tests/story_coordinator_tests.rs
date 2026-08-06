use aise::AiseConfig;
use aise::AiseEngine;
use aise::AiseError;
use aise::CoordinatorConfig;
use aise::TurnEvent;
use aise::TurnEventSink;
use aise::character::CharacterThinkPipeline;
use aise::context::BaselineContextBuilder;
use aise::context::ContextRetrievalPipeline;
use aise::core::turn_contract::{ExecuteTurnSpec, IdempotencyKey, TurnCancellation};
use aise::domain::ids::StoryId;
use aise::engine::{SystemClock, UuidIdGenerator};
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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct SlowProvider {
    delay: Duration,
    active: Arc<AtomicUsize>,
    max_active: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl LlmProvider for SlowProvider {
    fn provider_name(&self) -> &'static str {
        "slow"
    }

    async fn complete(&self, req: &CompletionRequest) -> Result<LlmCompletion, LlmError> {
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_active.fetch_max(active, Ordering::SeqCst);
        tokio::time::sleep(self.delay).await;
        self.active.fetch_sub(1, Ordering::SeqCst);
        let text = match req.purpose {
            "writer_plan" => {
                r#"{"retrieval_requests":[],"character_requests":[],"story_goal":{"summary":""}}"#.to_string()
            }
            "story_generation" | "story_repair" => r#"{"story_text":"story text","events":[{"kind":"action","summary":"story text"}],"character_changes":[],"world_change":{"add_facts":[]},"memory_changes":[],"summary_delta":null}"#.to_string(),
            _ => "story text".to_string(),
        };
        Ok(LlmCompletion {
            text,
            finish_reason: Some(FinishReason::Stop),
            reasoning_content: None,
            usage: None,
            charge: None,
        })
    }

    async fn complete_stream(&self, _req: &CompletionRequest, _on_delta: DeltaSink) -> Result<LlmCompletion, LlmError> {
        self.complete(_req).await
    }

    async fn embed(&self, _req: &EmbeddingRequest) -> Result<EmbeddingOutput, LlmError> {
        Err(LlmError::EmbeddingUnsupported)
    }
}

#[derive(Default)]
struct Recorder {
    events: Mutex<Vec<TurnEvent>>,
}

impl TurnEventSink for Recorder {
    fn emit(&self, event: TurnEvent) {
        self.events.lock().unwrap().push(event);
    }
}

struct TestEngine {
    engine: Arc<AiseEngine>,
    max_active: Arc<AtomicUsize>,
}

async fn build_engine(db_url: &str, delay: Duration) -> TestEngine {
    let store = SqliteStore::connect(db_url).await.expect("connect store");
    let config = AiseConfig::default();
    let max_active = Arc::new(AtomicUsize::new(0));
    let provider: Arc<dyn LlmProvider> = Arc::new(SlowProvider {
        delay,
        active: Arc::new(AtomicUsize::new(0)),
        max_active: max_active.clone(),
    });
    let gateway = Arc::new(LlmGateway::new(provider, config.llm.clone()).expect("gateway"));
    let coordinator = StoryTurnCoordinator::new(&config.coordinator);
    let pipeline_set = TurnPipelineSet::builder()
        .initializer(Box::<TurnInitializer>::default())
        .baseline_builder(Box::new(BaselineContextBuilder::new(store.clone())))
        .writer_planner(Box::new(WriterPlanner::new(gateway.clone())))
        .retrieval(Box::new(ContextRetrievalPipeline))
        .character_think(Box::new(CharacterThinkPipeline::new(gateway.clone())))
        .story_generator(Box::new(StoryGenerator::new(gateway.clone())))
        .validation(Box::new(ValidationPipeline::default()))
        .story_repairer(Box::new(StoryRepairer::new(gateway.clone())))
        .committer(Box::new(TurnCommitter::new(store.clone())))
        .build()
        .expect("pipeline set");
    let runtime = TurnRuntime::new(pipeline_set);
    TestEngine {
        engine: Arc::new(AiseEngine::new(
            runtime,
            store,
            coordinator,
            config,
            Arc::new(UuidIdGenerator),
            Arc::new(SystemClock),
        )),
        max_active,
    }
}

fn spec_for(story_id: &str, player_input: &str) -> ExecuteTurnSpec {
    ExecuteTurnSpec {
        story_id: StoryId::from(story_id),
        idempotency_key: IdempotencyKey::try_new("test-key".to_string()).unwrap(),
        player_input: player_input.to_string(),
        cancellation: TurnCancellation::new(),
    }
}

fn temp_db_path(label: &str) -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir()
        .join(format!("aise_{label}_{now}.db"))
        .to_string_lossy()
        .into_owned()
}

#[tokio::test]
async fn same_story_turns_never_overlap_through_engine_api() {
    let db = temp_db_path("coord_same");
    let test = build_engine(&db, Duration::from_millis(200)).await;
    let recorder1 = Arc::new(Recorder::default());
    let recorder2 = Arc::new(Recorder::default());

    let engine1 = test.engine.clone();
    let r1 = recorder1.clone();
    let mut first_spec = spec_for("story-same", "第一回合");
    first_spec.idempotency_key = IdempotencyKey::try_new("key-1".to_string()).unwrap();
    let first = tokio::spawn(async move { engine1.run_turn(first_spec, &*r1).await });
    let engine2 = test.engine.clone();
    let r2 = recorder2.clone();
    let mut second_spec = spec_for("story-same", "第二回合");
    second_spec.idempotency_key = IdempotencyKey::try_new("key-2".to_string()).unwrap();
    let second = tokio::spawn(async move { engine2.run_turn(second_spec, &*r2).await });

    let start = Instant::now();
    first.await.unwrap().expect("turn 1");
    second.await.unwrap().expect("turn 2");
    let elapsed = start.elapsed();

    assert!(
        elapsed >= Duration::from_millis(400),
        "same story must serialize, took {elapsed:?}"
    );
    assert_eq!(
        test.max_active.load(Ordering::SeqCst),
        1,
        "direct engine call must not bypass coordination"
    );
    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn direct_engine_call_cannot_bypass_coordination() {
    let db = temp_db_path("coord_direct");
    let test = build_engine(&db, Duration::from_millis(300)).await;
    let coordinator = test.engine.coordinator();

    let engine1 = test.engine.clone();
    let recorder1 = Arc::new(Recorder::default());
    let mut spec1 = spec_for("story-direct", "回合甲");
    spec1.idempotency_key = IdempotencyKey::try_new("key-1".to_string()).unwrap();
    let first = tokio::spawn(async move { engine1.run_turn(spec1, &*recorder1).await });

    let engine2 = test.engine.clone();
    let recorder2 = Arc::new(Recorder::default());
    let mut spec2 = spec_for("story-direct", "回合乙");
    spec2.idempotency_key = IdempotencyKey::try_new("key-2".to_string()).unwrap();
    let second = tokio::spawn(async move { engine2.run_turn(spec2, &*recorder2).await });

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(coordinator.active_permits(), 1, "the first direct call holds the story permit");
    assert_eq!(
        coordinator.total_waiters(),
        1,
        "the second direct call must queue on the coordinator"
    );

    first.await.unwrap().expect("turn 1");
    second.await.unwrap().expect("turn 2");
    assert_eq!(
        test.max_active.load(Ordering::SeqCst),
        1,
        "direct engine calls must never run the same story concurrently"
    );
    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn different_story_turns_can_overlap() {
    let db = temp_db_path("coord_parallel");
    let test = build_engine(&db, Duration::from_millis(100)).await;
    let recorder1 = Arc::new(Recorder::default());
    let recorder2 = Arc::new(Recorder::default());

    let engine1 = test.engine.clone();
    let r1 = recorder1.clone();
    let first = tokio::spawn(async move { engine1.run_turn(spec_for("story-a", "第一回合"), &*r1).await });
    let engine2 = test.engine.clone();
    let r2 = recorder2.clone();
    let second = tokio::spawn(async move { engine2.run_turn(spec_for("story-b", "第二回合"), &*r2).await });

    let start = Instant::now();
    first.await.unwrap().expect("turn a");
    second.await.unwrap().expect("turn b");
    let elapsed = start.elapsed();

    assert_eq!(
        test.max_active.load(Ordering::SeqCst),
        2,
        "different stories must run concurrently inside the provider"
    );
    assert!(
        elapsed < Duration::from_millis(500),
        "turns must overlap in wall-clock time, took {elapsed:?}"
    );
    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn story_wait_queue_rejects_over_capacity() {
    let config = CoordinatorConfig {
        max_waiters_per_story: 1,
        max_total_waiters: 4,
        ..CoordinatorConfig::default()
    };
    let coordinator = StoryTurnCoordinator::new(&config);
    let story = StoryId::from("story-cap");
    let deadline = Instant::now() + Duration::from_secs(10);
    let cancellation = TurnCancellation::new();

    let holder = coordinator
        .acquire(&story, deadline, &cancellation)
        .await
        .expect("holder permit");
    let co = coordinator.clone();
    let s = story.clone();
    let waiter_cancellation = cancellation.clone();
    let waiter = tokio::spawn(async move { co.acquire(&s, deadline, &waiter_cancellation).await });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(coordinator.total_waiters(), 1, "one waiter queued");

    let error = coordinator
        .acquire(&story, deadline, &cancellation)
        .await
        .expect_err("per-story waiter cap must reject");
    assert!(matches!(error, AiseError::Backpressure(_)));

    drop(holder);
    waiter.await.unwrap().expect("queued waiter proceeds after release");
    assert_eq!(coordinator.total_waiters(), 0);
}

#[tokio::test]
async fn story_wait_queue_rejects_global_capacity() {
    let config = CoordinatorConfig {
        max_waiters_per_story: 8,
        max_total_waiters: 2,
        ..CoordinatorConfig::default()
    };
    let coordinator = StoryTurnCoordinator::new(&config);
    let story_a = StoryId::from("story-a");
    let story_b = StoryId::from("story-b");
    let deadline = Instant::now() + Duration::from_secs(10);
    let cancellation = TurnCancellation::new();

    let holder_a = coordinator.acquire(&story_a, deadline, &cancellation).await.expect("holder a");
    let holder_b = coordinator.acquire(&story_b, deadline, &cancellation).await.expect("holder b");
    let co = coordinator.clone();
    let cancellation_a = cancellation.clone();
    let waiter_a = tokio::spawn(async move { co.acquire(&story_a, deadline, &cancellation_a).await });
    let co = coordinator.clone();
    let cancellation_b = cancellation.clone();
    let waiter_b = tokio::spawn(async move { co.acquire(&story_b, deadline, &cancellation_b).await });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(coordinator.total_waiters(), 2, "two waiters queued");

    let error = coordinator
        .acquire(&StoryId::from("story-c"), deadline, &cancellation)
        .await
        .expect_err("global waiter cap must reject");
    assert!(matches!(error, AiseError::Backpressure(_)));

    drop(holder_a);
    drop(holder_b);
    waiter_a.await.unwrap().expect("waiter a proceeds");
    waiter_b.await.unwrap().expect("waiter b proceeds");
    assert_eq!(coordinator.total_waiters(), 0);
}

#[tokio::test]
async fn coordinator_reclaims_idle_story_entries() {
    let config = CoordinatorConfig {
        idle_timeout_secs: 1,
        ..CoordinatorConfig::default()
    };
    let coordinator = StoryTurnCoordinator::new(&config);
    let story = StoryId::from("story-idle");
    let deadline = Instant::now() + Duration::from_secs(10);
    let cancellation = TurnCancellation::new();

    let permit = coordinator.acquire(&story, deadline, &cancellation).await.expect("permit");
    drop(permit);
    assert_eq!(coordinator.entry_count(), 1, "entry retained while within idle timeout");

    tokio::time::sleep(Duration::from_millis(1_200)).await;
    coordinator.reclaim_idle();
    assert_eq!(coordinator.entry_count(), 0, "idle entry reclaimed after timeout");

    let _permit = coordinator
        .acquire(&story, deadline, &cancellation)
        .await
        .expect("entry recreated");
    assert_eq!(coordinator.entry_count(), 1);
}

#[tokio::test]
async fn coordinator_shutdown_rejects_new_acquires_and_cancels_waiters() {
    let coordinator = StoryTurnCoordinator::new(&CoordinatorConfig::default());
    let story = StoryId::from("story-sd");
    let deadline = Instant::now() + Duration::from_secs(10);
    let cancellation = TurnCancellation::new();

    let holder = coordinator.acquire(&story, deadline, &cancellation).await.expect("holder");
    let co = coordinator.clone();
    let s = story.clone();
    let waiter_cancellation = cancellation.clone();
    let waiter = tokio::spawn(async move { co.acquire(&s, deadline, &waiter_cancellation).await });
    tokio::time::sleep(Duration::from_millis(50)).await;

    coordinator.shutdown();

    let error = coordinator
        .acquire(&StoryId::from("story-sd2"), deadline, &cancellation)
        .await
        .expect_err("new acquire after shutdown must be rejected");
    assert!(matches!(error, AiseError::Backpressure(_)));

    let waiter_error = waiter.await.unwrap().expect_err("queued waiter cancelled by shutdown");
    assert!(matches!(waiter_error, AiseError::Backpressure(_)));
    drop(holder);
    assert_eq!(coordinator.active_permits(), 0);
}

#[tokio::test]
async fn coordinator_shutdown_waits_for_active_turns_within_grace() {
    let coordinator = StoryTurnCoordinator::new(&CoordinatorConfig::default());
    let story = StoryId::from("story-grace");
    let deadline = Instant::now() + Duration::from_secs(10);
    let cancellation = TurnCancellation::new();
    let holder = coordinator.acquire(&story, deadline, &cancellation).await.expect("holder");

    let co = coordinator.clone();
    let shutdown = tokio::spawn(async move { co.shutdown_with_grace(Duration::from_millis(500)).await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(coordinator.active_permits(), 1, "active turn keeps running during grace");

    drop(holder);
    tokio::time::timeout(Duration::from_secs(2), shutdown)
        .await
        .expect("shutdown completes once active turn finishes")
        .unwrap();
    assert_eq!(coordinator.active_permits(), 0);
}
