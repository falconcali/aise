use aise::AiseConfig;
use aise::AiseEngine;
use aise::character::CharacterThinkPipeline;
use aise::context::BaselineContextBuilder;
use aise::context::ContextRetrievalPipeline;
use aise::domain::ids::StoryId;
use aise::engine::TurnEvent;
use aise::llm::error::LlmError;
use aise::llm::provider::DeltaSink;
use aise::llm::provider::LlmProvider;
use aise::persistence::SqliteStore;
use aise::persistence::TurnCommitter;
use aise::planning::WriterPlanner;
use aise::runtime::TurnEventSink;
use aise::runtime::TurnInitializer;
use aise::runtime::TurnRuntime;
use aise::story::StoryGenerator;
use aise::validation::ValidationPipeline;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

struct StubLlm;

#[async_trait::async_trait]
impl LlmProvider for StubLlm {
    async fn complete(&self, _req: &aise::llm::message::CompletionRequest) -> Result<String, LlmError> {
        Ok("Hello World".to_string())
    }

    async fn complete_stream(
        &self,
        _req: &aise::llm::message::CompletionRequest,
        _on_delta: DeltaSink,
    ) -> Result<(), LlmError> {
        Ok(())
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
    let llm: Arc<dyn LlmProvider> = Arc::new(StubLlm);
    let config = AiseConfig::default();
    let runtime = TurnRuntime::new(vec![
        Box::<TurnInitializer>::default(),
        Box::new(BaselineContextBuilder::new(store.clone())),
        Box::new(WriterPlanner),
        Box::new(ContextRetrievalPipeline),
        Box::new(CharacterThinkPipeline),
        Box::new(StoryGenerator::new(llm.clone(), &config.llm, config.turn.max_tokens)),
        Box::new(ValidationPipeline::default()),
        Box::new(TurnCommitter::new(store.clone())),
    ]);
    Arc::new(AiseEngine::new(runtime, store, llm, config))
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
    let story_id = StoryId::from("story-1");

    let result = engine
        .run_turn(&story_id, "开始吧".to_string(), &recorder)
        .await
        .expect("run turn");

    assert_eq!(result.story_text, "Hello World");
    assert!(!result.turn_id.as_str().is_empty());

    {
        let events = recorder.events.lock().unwrap();
        assert_eq!(events.len(), 12);
        assert!(events.iter().any(|e| matches!(e, TurnEvent::StageStarted("turn_initializer"))));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, TurnEvent::StageStarted("baseline_ctx_builder")))
        );
        assert!(events.iter().any(|e| matches!(e, TurnEvent::StageStarted("writer_planner"))));
        assert!(events.iter().any(|e| matches!(e, TurnEvent::StageStarted("context_retrieval"))));
        assert!(events.iter().any(|e| matches!(e, TurnEvent::StageStarted("character_think"))));
        assert!(events.iter().any(|e| matches!(e, TurnEvent::StageStarted("story_generator"))));
        assert!(events.iter().any(|e| matches!(e, TurnEvent::StageStarted("validation"))));
        assert!(events.iter().any(|e| matches!(e, TurnEvent::StageStarted("turn_committer"))));
        assert!(events.iter().any(|e| matches!(e, TurnEvent::Validation { pass: true })));
        assert!(events.iter().any(|e| matches!(e, TurnEvent::Token(t) if t == "Hello World")));
        assert!(events.iter().any(|e| matches!(e, TurnEvent::Finished { .. })));
        assert!(events.iter().any(|e| matches!(e, TurnEvent::Trace(_))));
    }

    let store = engine.store();
    let turns = store.load_story(&story_id, 10).await.expect("load story");
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].story_text, "Hello World");
    assert_eq!(turns[0].player_input, "开始吧");

    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn turn_trace_records_llm_prompt_and_response() {
    let db = temp_db_path("trace");
    let engine = build_engine(&db).await;
    let recorder = Recorder::default();
    let story_id = StoryId::from("story-trace");

    let result = engine
        .run_turn(&story_id, "请讲一个故事".to_string(), &recorder)
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
    assert_eq!(payload.get("response").and_then(|v| v.as_str()), Some("Hello World"));
    let messages = payload.get("messages").and_then(|v| v.as_array()).expect("messages");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].get("role").and_then(|v| v.as_str()), Some("user"));
    assert_eq!(messages[0].get("content").and_then(|v| v.as_str()), Some("请讲一个故事"));

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
        .run_turn(&story_id, "第一回合".to_string(), &recorder)
        .await
        .expect("turn 1");
    engine
        .run_turn(&story_id, "第二回合".to_string(), &recorder)
        .await
        .expect("turn 2");

    let store = engine.store();
    let turns = store.load_story(&story_id, 10).await.expect("load story");
    assert_eq!(turns.len(), 2);
    assert_eq!(turns[0].player_input, "第二回合");
    assert_eq!(turns[1].player_input, "第一回合");

    let _ = std::fs::remove_file(&db);
}
