use aise::AiseConfig;
use aise::AiseEngine;
use aise::TurnEvent;
use aise::TurnEventSink;
use aise::character::CharacterThinkPipeline;
use aise::context::BaselineContextBuilder;
use aise::context::ContextRetrievalPipeline;
use aise::core::turn_contract::{ExecuteTurnSpec, IdempotencyKey, TurnCancellation};
use aise::domain::ids::StoryId;
use aise::llm::LlmGateway;
use aise::llm::accounting::{FinishReason, LlmCompletion};
use aise::llm::error::LlmError;
use aise::llm::message::{CompletionRequest, EmbeddingOutput, EmbeddingRequest};
use aise::llm::provider::{DeltaSink, LlmProvider};
use aise::persistence::{SqliteStore, TurnCommitter};
use aise::planning::WriterPlanner;
use aise::runtime::{StoryTurnCoordinator, TurnInitializer, TurnRuntime};
use aise::story::StoryGenerator;
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

    async fn complete(&self, _req: &CompletionRequest) -> Result<LlmCompletion, LlmError> {
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_active.fetch_max(active, Ordering::SeqCst);
        tokio::time::sleep(self.delay).await;
        self.active.fetch_sub(1, Ordering::SeqCst);
        Ok(LlmCompletion {
            text: "story text".into(),
            finish_reason: Some(FinishReason::Stop),
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
    let runtime = TurnRuntime::new(vec![
        Box::<TurnInitializer>::default(),
        Box::new(BaselineContextBuilder::new(store.clone())),
        Box::new(WriterPlanner),
        Box::new(ContextRetrievalPipeline),
        Box::new(CharacterThinkPipeline),
        Box::new(StoryGenerator::new(gateway.clone())),
        Box::new(ValidationPipeline::default()),
        Box::new(TurnCommitter::new(store.clone())),
    ]);
    TestEngine {
        engine: Arc::new(AiseEngine::new(runtime, store, coordinator, config)),
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
    let first = tokio::spawn(async move { engine1.run_turn(spec_for("story-same", "第一回合"), &*r1).await });
    let engine2 = test.engine.clone();
    let r2 = recorder2.clone();
    let second = tokio::spawn(async move { engine2.run_turn(spec_for("story-same", "第二回合"), &*r2).await });

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
async fn different_story_turns_can_overlap() {
    let db = temp_db_path("coord_parallel");
    let test = build_engine(&db, Duration::from_millis(200)).await;
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

    assert!(
        elapsed < Duration::from_millis(350),
        "different stories must overlap, took {elapsed:?}"
    );
    assert_eq!(test.max_active.load(Ordering::SeqCst), 2);
    let _ = std::fs::remove_file(&db);
}
