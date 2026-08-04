use aise::AiseConfig;
use aise::AiseEngine;
use aise::TurnEvent;
use aise::TurnEventSink;
use aise::character::CharacterThinkPipeline;
use aise::context::BaselineContextBuilder;
use aise::context::ContextRetrievalPipeline;
use aise::core::turn_contract::{ExecuteTurnSpec, IdempotencyKey, TurnCancellation};
use aise::core::turn_pipeline::TurnStage;
use aise::domain::ids::StoryId;
use aise::llm::LlmGateway;
use aise::llm::accounting::{FinishReason, LlmCompletion, LlmTokenUsage, UsageAccuracy};
use aise::llm::error::LlmError;
use aise::llm::message::{CompletionRequest, EmbeddingOutput, EmbeddingRequest};
use aise::llm::provider::{DeltaSink, LlmProvider};
use aise::persistence::SqliteStore;
use aise::persistence::TurnCommitter;
use aise::planning::WriterPlanner;
use aise::runtime::{StoryTurnCoordinator, TurnInitializer, TurnRuntime};
use aise::story::StoryGenerator;
use aise::validation::ValidationPipeline;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

struct StubLlm;

#[async_trait::async_trait]
impl LlmProvider for StubLlm {
    fn provider_name(&self) -> &'static str {
        "stub"
    }

    async fn complete(&self, _req: &CompletionRequest) -> Result<LlmCompletion, LlmError> {
        Ok(LlmCompletion {
            text: "Hello World".to_string(),
            finish_reason: Some(FinishReason::Stop),
            usage: Some(LlmTokenUsage {
                input_tokens: 10,
                cached_input_tokens: None,
                output_tokens: 20,
                total_tokens: 30,
                accuracy: UsageAccuracy::Exact,
            }),
            charge: None,
        })
    }

    async fn complete_stream(&self, _req: &CompletionRequest, _on_delta: DeltaSink) -> Result<LlmCompletion, LlmError> {
        Ok(LlmCompletion {
            text: String::new(),
            finish_reason: None,
            usage: None,
            charge: None,
        })
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

async fn build_engine(db_url: &str) -> Arc<AiseEngine> {
    let store = SqliteStore::connect(db_url).await.expect("connect store");
    let provider: Arc<dyn LlmProvider> = Arc::new(StubLlm);
    let config = AiseConfig::default();
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
    Arc::new(AiseEngine::new(runtime, store, coordinator, config))
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
async fn full_flow_returns_hello_world_and_persists() {
    let db = temp_db_path("flow");
    let engine = build_engine(&db).await;
    let recorder = Recorder::default();

    let result = engine
        .run_turn(spec_for("story-1", "开始吧"), &recorder)
        .await
        .expect("run turn");

    assert_eq!(result.story_text, "Hello World");
    assert!(!result.turn_id.as_str().is_empty());

    {
        let events = recorder.events.lock().unwrap();
        assert_eq!(events.len(), 12);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, TurnEvent::StageStarted(TurnStage::TurnInitializer)))
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, TurnEvent::StageStarted(TurnStage::BaselineBuilder)))
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, TurnEvent::StageStarted(TurnStage::WriterPlanner)))
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, TurnEvent::StageStarted(TurnStage::ContextRetrieval)))
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, TurnEvent::StageStarted(TurnStage::CharacterThink)))
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, TurnEvent::StageStarted(TurnStage::StoryGenerator)))
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, TurnEvent::StageStarted(TurnStage::Validation)))
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, TurnEvent::StageStarted(TurnStage::TurnCommitter)))
        );
        assert!(events.iter().any(|e| matches!(e, TurnEvent::Validation { pass: true })));
        assert!(events.iter().any(|e| matches!(e, TurnEvent::Token(t) if t == "Hello World")));
        assert!(events.iter().any(|e| matches!(e, TurnEvent::Finished { .. })));
        assert!(events.iter().any(|e| matches!(e, TurnEvent::Trace(_))));
    }

    let store = engine.store();
    let turns = store.load_story(&StoryId::from("story-1"), 10).await.expect("load story");
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].story_text, "Hello World");
    assert_eq!(turns[0].player_input, "开始吧");

    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn turn_trace_records_metadata_only_llm_usage() {
    let db = temp_db_path("trace");
    let engine = build_engine(&db).await;
    let recorder = Recorder::default();

    let result = engine
        .run_turn(spec_for("story-trace", "请讲一个故事"), &recorder)
        .await
        .expect("run turn");

    let events = recorder.events.lock().unwrap();
    let trace = events
        .iter()
        .find_map(|e| match e {
            TurnEvent::Trace(t) => Some(t.clone()),
            _ => None,
        })
        .expect("trace event");
    assert_eq!(trace.turn_id, result.turn_id.to_string());
    assert_eq!(trace.story_id, "story-trace");
    assert!(!trace.trace_id.is_empty());
    assert!(trace.duration_ms > 0);

    let kinds: Vec<_> = trace.spans.iter().map(|s| s.kind.as_str()).collect();
    assert!(kinds.contains(&"aise.turn"));
    assert!(kinds.contains(&"aise.pipeline"));
    assert!(kinds.contains(&"aise.llm_call"));
    assert!(kinds.contains(&"aise.tool_call"));
    assert!(kinds.contains(&"aise.validation"));
    assert!(kinds.contains(&"aise.persist"));

    let llm = trace.spans.iter().find(|s| s.kind == "aise.llm_call").expect("llm span");
    let payload = llm.payload.as_object().expect("llm payload object");
    assert_eq!(payload.get("status").and_then(|v| v.as_str()), Some("ok"));
    assert_eq!(payload.get("provider").and_then(|v| v.as_str()), Some("stub"));
    assert_eq!(payload.get("model").and_then(|v| v.as_str()), Some("qwen2.5"));
    assert_eq!(payload.get("input_tokens").and_then(|v| v.as_u64()), Some(10));
    assert_eq!(payload.get("output_tokens").and_then(|v| v.as_u64()), Some(20));
    assert_eq!(payload.get("usage_accuracy").and_then(|v| v.as_str()), Some("exact"));
    assert!(payload.get("content").is_none());

    let root = trace.spans.iter().find(|s| s.kind == "aise.turn").expect("root span");
    let root_payload = root.payload.as_object().expect("root payload");
    assert_eq!(root_payload.get("status").and_then(|v| v.as_str()), Some("ok"));
    assert_eq!(root_payload.get("player_input").and_then(|v| v.as_str()), Some("请讲一个故事"));

    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn second_turn_loads_history_from_store() {
    let db = temp_db_path("history");
    let engine = build_engine(&db).await;
    let recorder = Recorder::default();
    let story_id = StoryId::from("story-2");

    engine
        .run_turn(spec_for("story-2", "第一回合"), &recorder)
        .await
        .expect("turn 1");
    engine
        .run_turn(spec_for("story-2", "第二回合"), &recorder)
        .await
        .expect("turn 2");

    let store = engine.store();
    let turns = store.load_story(&story_id, 10).await.expect("load story");
    assert_eq!(turns.len(), 2);
    assert_eq!(turns[0].player_input, "第二回合");
    assert_eq!(turns[1].player_input, "第一回合");

    let _ = std::fs::remove_file(&db);
}
