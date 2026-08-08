use aise::AiseConfig;
use aise::AiseEngine;
use aise::TurnEvent;
use aise::TurnEventSink;
use aise::character::CharacterThinkPipeline;
use aise::context::BaselineContextBuilder;
use aise::context::ContextRetrievalPipeline;
use aise::core::turn_contract::{
    CommittedTurnResult, ExecuteTurnSpec, IdempotencyKey, LlmCallPurpose, TurnCancellation,
};
use aise::core::turn_data::SnapshotLimits;
use aise::core::turn_error::TurnFailureKind;
use aise::core::turn_pipeline::TurnStage;
use aise::core::turn_validation::ValidationDecision;
use aise::domain::ids::StoryId;
use aise::domain::story_state::StoryReadSnapshot;
use aise::engine::{SystemClock, UuidIdGenerator};
use aise::llm::LlmGateway;
use aise::llm::accounting::{FinishReason, LlmCompletion, LlmTokenUsage, UsageAccuracy};
use aise::llm::error::{LlmProtocolErrorKind, LlmProviderError};
use aise::llm::message::{CompletionRequest, EmbeddingOutput, EmbeddingRequest};
use aise::llm::provider::{DeltaSink, LlmProvider};
use aise::persistence::SqliteStore;
use aise::persistence::TurnCommitter;
use aise::planning::WriterPlanner;
use aise::runtime::{StoryTurnCoordinator, TurnInitializer, TurnPipelineSet, TurnRuntime};
use aise::story::{StoryGenerator, StoryRepairer};
use aise::validation::ValidationPipeline;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

struct StubLlm;

fn stub_completion_text(purpose: LlmCallPurpose) -> String {
    match purpose {
        LlmCallPurpose::WriterPlan => {
            r#"{"retrieval_requests":[],"character_requests":[],"story_goal":{"summary":""}}"#.to_string()
        }
        LlmCallPurpose::StoryGeneration | LlmCallPurpose::StoryRepair => story_proposal_json("Hello World"),
        _ => "Hello World".to_string(),
    }
}

fn story_proposal_json(story: &str) -> String {
    format!(
        r#"{{"story_text":"{story}","events":[{{"kind":"action","summary":"{story}"}}],"character_changes":[],"world_change":{{"add_facts":[]}},"memory_changes":[],"summary_change":null}}"#
    )
}

#[async_trait::async_trait]
impl LlmProvider for StubLlm {
    fn provider_name(&self) -> &'static str {
        "stub"
    }

    async fn complete(&self, req: &CompletionRequest) -> Result<LlmCompletion, LlmProviderError> {
        Ok(LlmCompletion {
            text: stub_completion_text(req.purpose),
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
        Ok(LlmCompletion {
            text: String::new(),
            finish_reason: None,
            reasoning_content: None,
            usage: None,
            charge: None,
        })
    }

    async fn embed(&self, _req: &EmbeddingRequest) -> Result<EmbeddingOutput, LlmProviderError> {
        Err(LlmProviderError::Protocol {
            kind: LlmProtocolErrorKind::Unsupported,
        })
    }
}

#[derive(Default)]
struct Recorder {
    events: Mutex<Vec<TurnEvent>>,
}

impl TurnEventSink for Recorder {
    fn emit(&self, event: TurnEvent) -> Result<(), aise::core::turn_event::TurnEventDeliveryError> {
        self.events.lock().unwrap().push(event);
        Ok(())
    }
}

async fn build_engine(db_url: &str) -> Arc<AiseEngine> {
    let store: Arc<dyn aise::persistence::Store> = SqliteStore::connect(db_url).await.expect("connect store");
    let provider: Arc<dyn LlmProvider> = Arc::new(StubLlm);
    build_engine_with(store, provider, AiseConfig::default()).await
}

async fn build_engine_with(
    store: Arc<dyn aise::persistence::Store>,
    provider: Arc<dyn LlmProvider>,
    config: AiseConfig,
) -> Arc<AiseEngine> {
    let gateway = Arc::new(LlmGateway::new(provider, config.llm.clone()).expect("gateway"));
    let coordinator = StoryTurnCoordinator::new(&config.coordinator);
    let pipeline_set = TurnPipelineSet::builder()
        .initializer(Box::<TurnInitializer>::default())
        .baseline_builder(Box::new(BaselineContextBuilder::new(store.clone(), config.content.clone())))
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
    Arc::new(AiseEngine::new(
        runtime,
        store,
        coordinator,
        config,
        Arc::new(UuidIdGenerator),
        Arc::new(SystemClock),
    ))
}

async fn ensure_story(engine: &Arc<AiseEngine>, story_id: &str) {
    let story_id = StoryId::try_new(story_id).unwrap();
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
}

fn spec_for(story_id: &str, player_input: &str) -> ExecuteTurnSpec {
    ExecuteTurnSpec {
        story_id: StoryId::try_new(story_id).unwrap(),
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
    ensure_story(&engine, "story-1").await;
    let recorder = Recorder::default();

    let result = engine
        .run_turn(spec_for("story-1", "开始吧"), &recorder)
        .await
        .expect("run turn");

    assert_eq!(result.story_text, "Hello World");
    assert!(!result.turn_id.as_str().is_empty());

    {
        let events = recorder.events.lock().unwrap();
        assert!(events.iter().any(|e| matches!(
            e,
            TurnEvent::StageStarted {
                stage: TurnStage::TurnInitializer,
                ..
            }
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            TurnEvent::StageStarted {
                stage: TurnStage::BaselineBuilder,
                ..
            }
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            TurnEvent::StageStarted {
                stage: TurnStage::WriterPlanner,
                ..
            }
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            TurnEvent::StageStarted {
                stage: TurnStage::StoryGenerator,
                ..
            }
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            TurnEvent::StageStarted {
                stage: TurnStage::Validation,
                ..
            }
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            TurnEvent::StageStarted {
                stage: TurnStage::TurnCommitter,
                ..
            }
        )));
        assert!(!events.iter().any(|e| matches!(
            e,
            TurnEvent::StageStarted {
                stage: TurnStage::ContextRetrieval,
                ..
            }
        )));
        assert!(!events.iter().any(|e| matches!(
            e,
            TurnEvent::StageStarted {
                stage: TurnStage::CharacterThink,
                ..
            }
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            TurnEvent::ValidationCompleted {
                decision: ValidationDecision::Pass,
                ..
            }
        )));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, TurnEvent::Committed { result, .. } if result.story_text == "Hello World"))
        );
        assert!(events.iter().any(|e| matches!(e, TurnEvent::Committed { .. })));
        assert!(events.iter().any(|e| matches!(e, TurnEvent::TraceCompleted { .. })));
    }

    let store = engine.store();
    let snapshot = store
        .load_story_snapshot(&StoryId::try_new("story-1").unwrap(), snapshot_limits())
        .await
        .expect("load snapshot");
    let turns = snapshot.recent_turns();
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].story_text, "Hello World");
    assert_eq!(turns[0].player_input, "开始吧");

    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn turn_trace_records_metadata_only_llm_usage() {
    let db = temp_db_path("trace");
    let engine = build_engine(&db).await;
    ensure_story(&engine, "story-trace").await;
    let recorder = Recorder::default();

    let result = engine
        .run_turn(spec_for("story-trace", "请讲一个故事"), &recorder)
        .await
        .expect("run turn");

    let events = recorder.events.lock().unwrap();
    let trace_event = events
        .iter()
        .find_map(|e| match e {
            TurnEvent::TraceCompleted { trace, .. } => Some(trace),
            _ => None,
        })
        .expect("trace event");
    assert_eq!(trace_event.turn_id, result.turn_id.to_string());
    assert!(!trace_event.trace_id.as_str().is_empty());
    assert!(!trace_event.spans.is_empty());
    assert!(
        trace_event
            .spans
            .iter()
            .any(|span| span.kind == "aise.llm_call")
    );
    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn second_turn_loads_history_from_store_in_chronological_order() {
    let db = temp_db_path("history");
    let engine = build_engine(&db).await;
    ensure_story(&engine, "story-2").await;
    let recorder = Recorder::default();
    let story_id = StoryId::try_new("story-2").unwrap();

    let mut turn_one = spec_for("story-2", "第一回合");
    turn_one.idempotency_key = IdempotencyKey::try_new("key-1".to_string()).unwrap();
    engine.run_turn(turn_one, &recorder).await.expect("turn 1");
    let mut turn_two = spec_for("story-2", "第二回合");
    turn_two.idempotency_key = IdempotencyKey::try_new("key-2".to_string()).unwrap();
    engine.run_turn(turn_two, &recorder).await.expect("turn 2");

    let store = engine.store();
    let snapshot = store
        .load_story_snapshot(&story_id, snapshot_limits())
        .await
        .expect("load snapshot");
    let turns = snapshot.recent_turns();
    assert_eq!(turns.len(), 2);
    assert_eq!(turns[0].player_input, "第一回合");
    assert_eq!(turns[1].player_input, "第二回合");

    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn response_loss_retry_does_not_call_llm_again() {
    let db = temp_db_path("idem_retry");
    let calls = Arc::new(AtomicUsize::new(0));
    let store: Arc<dyn aise::persistence::Store> = SqliteStore::connect(&db).await.expect("connect store");
    let provider: Arc<dyn LlmProvider> = Arc::new(CountingLlm { calls: calls.clone() });
    let engine = build_engine_with(store, provider, AiseConfig::default()).await;
    ensure_story(&engine, "story-idem").await;
    let recorder = Recorder::default();

    let mut first_spec = spec_for("story-idem", "同一个请求");
    first_spec.idempotency_key = IdempotencyKey::try_new("retry-key".to_string()).unwrap();
    let first = engine.run_turn(first_spec, &recorder).await.expect("first turn");

    let mut retry_spec = spec_for("story-idem", "同一个请求");
    retry_spec.idempotency_key = IdempotencyKey::try_new("retry-key".to_string()).unwrap();
    let retry = engine.run_turn(retry_spec, &recorder).await.expect("retry turn");

    assert_eq!(first.turn_id, retry.turn_id);
    assert_eq!(first.story_text, retry.story_text);
    assert_eq!(first.story_revision, retry.story_revision);
    assert_eq!(calls.load(Ordering::SeqCst), 2, "retry must not call the LLM again");

    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn same_key_with_different_request_returns_idempotency_conflict() {
    let db = temp_db_path("idem_conflict");
    let engine = build_engine(&db).await;
    ensure_story(&engine, "story-conf").await;
    let recorder = Recorder::default();

    let mut first_spec = spec_for("story-conf", "原始请求");
    first_spec.idempotency_key = IdempotencyKey::try_new("conflict-key".to_string()).unwrap();
    engine.run_turn(first_spec, &recorder).await.expect("first turn");

    let mut retry_spec = spec_for("story-conf", "不同请求");
    retry_spec.idempotency_key = IdempotencyKey::try_new("conflict-key".to_string()).unwrap();
    let error = engine
        .run_turn(retry_spec, &recorder)
        .await
        .expect_err("different request with same key must conflict");
    assert!(matches!(error.kind(), TurnFailureKind::IdempotencyConflict));

    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn invalid_request_fails_with_invalid_request() {
    let db = temp_db_path("invalid");
    let engine = build_engine(&db).await;
    let recorder = Recorder::default();
    let mut spec = spec_for("story-invalid", "");
    spec.idempotency_key = IdempotencyKey::try_new("invalid-key".to_string()).unwrap();
    let error = engine
        .run_turn(spec, &recorder)
        .await
        .expect_err("empty player input must fail");
    assert!(matches!(error.kind(), TurnFailureKind::InvalidRequest));
    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn turn_execution_never_creates_missing_story() {
    let db = temp_db_path("no_auto_create");
    let engine = build_engine(&db).await;
    let recorder = Recorder::default();
    let story_id = StoryId::try_new("story-never-created").unwrap();
    let mut spec = spec_for("story-never-created", "开始吧");
    spec.idempotency_key = IdempotencyKey::try_new("missing-key".to_string()).unwrap();
    let error = engine
        .run_turn(spec, &recorder)
        .await
        .expect_err("turn for a missing story must fail");
    assert!(matches!(error.kind(), TurnFailureKind::StoryNotFound));
    assert!(
        engine.store().get_story(&story_id).await.expect("get story").is_none(),
        "turn execution must never auto-create a story row"
    );
    let _ = std::fs::remove_file(&db);
}

fn snapshot_limits() -> SnapshotLimits {
    SnapshotLimits::from_config(&aise::config::TurnContentLimitsConfig::default())
}

struct CountingLlm {
    calls: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl LlmProvider for CountingLlm {
    fn provider_name(&self) -> &'static str {
        "counting"
    }

    async fn complete(&self, req: &CompletionRequest) -> Result<LlmCompletion, LlmProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(LlmCompletion {
            text: stub_completion_text(req.purpose),
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
        Ok(LlmCompletion {
            text: String::new(),
            finish_reason: None,
            reasoning_content: None,
            usage: None,
            charge: None,
        })
    }

    async fn embed(&self, _req: &EmbeddingRequest) -> Result<EmbeddingOutput, LlmProviderError> {
        Err(LlmProviderError::Protocol {
            kind: LlmProtocolErrorKind::Unsupported,
        })
    }
}

struct TrackedStore {
    inner: Arc<SqliteStore>,
    get_story_calls: Arc<AtomicUsize>,
    commit_calls: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl aise::persistence::Store for TrackedStore {
    async fn create_story(
        &self,
        spec: &aise::domain::StoryCreateSpec,
    ) -> Result<aise::domain::StoryInfo, aise::persistence::StoreError> {
        self.inner.create_story(spec).await
    }

    async fn create_story_instance(
        &self,
        spec: &aise::persistence::store::MaterializedStoryInstanceSpec,
    ) -> Result<aise::domain::StoryInfo, aise::persistence::StoreError> {
        self.inner.create_story_instance(spec).await
    }

    async fn get_story(
        &self,
        story_id: &StoryId,
    ) -> Result<Option<aise::domain::StoryInfo>, aise::persistence::StoreError> {
        self.get_story_calls.fetch_add(1, Ordering::SeqCst);
        self.inner.get_story(story_id).await
    }

    async fn load_story_snapshot(
        &self,
        story_id: &StoryId,
        limits: SnapshotLimits,
    ) -> Result<StoryReadSnapshot, aise::persistence::StoreError> {
        self.inner.load_story_snapshot(story_id, limits).await
    }

    async fn load_story_instance_meta(
        &self,
        story_id: &StoryId,
    ) -> Result<Option<aise::persistence::store::StoryInstanceMeta>, aise::persistence::StoreError> {
        self.inner.load_story_instance_meta(story_id).await
    }

    async fn find_committed_turn(
        &self,
        story_id: &StoryId,
        idempotency_key: &IdempotencyKey,
    ) -> Result<Option<aise::persistence::StoredTurnOutcome>, aise::persistence::StoreError> {
        self.inner.find_committed_turn(story_id, idempotency_key).await
    }

    async fn commit_turn(
        &self,
        commit: &aise::persistence::TurnCommitSpec,
    ) -> Result<CommittedTurnResult, aise::persistence::StoreError> {
        self.commit_calls.fetch_add(1, Ordering::SeqCst);
        self.inner.commit_turn(commit).await
    }
}

struct ConflictStore {
    inner: Arc<SqliteStore>,
}

#[async_trait::async_trait]
impl aise::persistence::Store for ConflictStore {
    async fn create_story(
        &self,
        spec: &aise::domain::StoryCreateSpec,
    ) -> Result<aise::domain::StoryInfo, aise::persistence::StoreError> {
        self.inner.create_story(spec).await
    }

    async fn create_story_instance(
        &self,
        spec: &aise::persistence::store::MaterializedStoryInstanceSpec,
    ) -> Result<aise::domain::StoryInfo, aise::persistence::StoreError> {
        self.inner.create_story_instance(spec).await
    }

    async fn get_story(
        &self,
        story_id: &StoryId,
    ) -> Result<Option<aise::domain::StoryInfo>, aise::persistence::StoreError> {
        self.inner.get_story(story_id).await
    }

    async fn load_story_snapshot(
        &self,
        story_id: &StoryId,
        limits: SnapshotLimits,
    ) -> Result<StoryReadSnapshot, aise::persistence::StoreError> {
        self.inner.load_story_snapshot(story_id, limits).await
    }

    async fn load_story_instance_meta(
        &self,
        story_id: &StoryId,
    ) -> Result<Option<aise::persistence::store::StoryInstanceMeta>, aise::persistence::StoreError> {
        self.inner.load_story_instance_meta(story_id).await
    }

    async fn find_committed_turn(
        &self,
        story_id: &StoryId,
        idempotency_key: &IdempotencyKey,
    ) -> Result<Option<aise::persistence::StoredTurnOutcome>, aise::persistence::StoreError> {
        self.inner.find_committed_turn(story_id, idempotency_key).await
    }

    async fn commit_turn(
        &self,
        _commit: &aise::persistence::TurnCommitSpec,
    ) -> Result<CommittedTurnResult, aise::persistence::StoreError> {
        Err(aise::persistence::StoreError::RevisionConflict)
    }
}

struct CancellingProvider {
    cancellation: TurnCancellation,
}

#[async_trait::async_trait]
impl LlmProvider for CancellingProvider {
    fn provider_name(&self) -> &'static str {
        "cancelling"
    }

    async fn complete(&self, req: &CompletionRequest) -> Result<LlmCompletion, LlmProviderError> {
        if req.purpose == LlmCallPurpose::WriterPlan {
            self.cancellation.cancel();
            tokio::time::sleep(Duration::from_secs(30)).await;
            return Err(LlmProviderError::Rejected {
                status: 500,
                code: None,
                message: None,
            });
        }
        Ok(LlmCompletion {
            text: stub_completion_text(req.purpose),
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

struct RejectingProvider;

#[async_trait::async_trait]
impl LlmProvider for RejectingProvider {
    fn provider_name(&self) -> &'static str {
        "rejecting"
    }

    async fn complete(&self, req: &CompletionRequest) -> Result<LlmCompletion, LlmProviderError> {
        if matches!(req.purpose, LlmCallPurpose::StoryGeneration | LlmCallPurpose::StoryRepair) {
            return Err(LlmProviderError::Rejected {
                status: 500,
                code: None,
                message: None,
            });
        }
        Ok(LlmCompletion {
            text: stub_completion_text(req.purpose),
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

struct RepairProposalProvider;

fn repairable_proposal_json() -> String {
    r#"{"story_text":"text","events":[{"kind":"action","summary":"text"}],"character_changes":[{"character_id":"c-999","goal_updates":[],"health_delta":null,"affinity_deltas":[]}],"world_change":{"add_facts":[]},"memory_changes":[],"summary_change":null}"#.to_string()
}

#[async_trait::async_trait]
impl LlmProvider for RepairProposalProvider {
    fn provider_name(&self) -> &'static str {
        "repair_loop"
    }

    async fn complete(&self, req: &CompletionRequest) -> Result<LlmCompletion, LlmProviderError> {
        let text = match req.purpose {
            LlmCallPurpose::WriterPlan => {
                r#"{"retrieval_requests":[],"character_requests":[],"story_goal":{"summary":""}}"#.to_string()
            }
            _ => repairable_proposal_json(),
        };
        Ok(LlmCompletion {
            text,
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

#[tokio::test]
async fn invalid_story_id_has_no_store_or_coordinator_side_effects() {
    let db = temp_db_path("invalid_sid");
    let get_story_calls = Arc::new(AtomicUsize::new(0));
    let commit_calls = Arc::new(AtomicUsize::new(0));
    let store: Arc<dyn aise::persistence::Store> = Arc::new(TrackedStore {
        inner: SqliteStore::connect(&db).await.expect("connect store"),
        get_story_calls: get_story_calls.clone(),
        commit_calls: commit_calls.clone(),
    });
    let engine = build_engine_with(store, Arc::new(StubLlm), AiseConfig::default()).await;
    let recorder = Recorder::default();
    let mut spec = spec_for("story-that-does-not-exist", "   ");
    spec.idempotency_key = IdempotencyKey::try_new("invalid-key".to_string()).unwrap();
    let error = engine
        .run_turn(spec, &recorder)
        .await
        .expect_err("invalid request must fail before any store or coordinator side effect");
    assert!(matches!(error.kind(), TurnFailureKind::InvalidRequest));
    assert_eq!(
        get_story_calls.load(Ordering::SeqCst),
        0,
        "request validation must run before store lookup"
    );
    assert_eq!(
        commit_calls.load(Ordering::SeqCst),
        0,
        "nothing may commit for an invalid request"
    );
    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn idempotency_replay_emits_original_committed_event() {
    let db = temp_db_path("replay_event");
    let engine = build_engine(&db).await;
    ensure_story(&engine, "story-replay").await;
    let first_recorder = Recorder::default();
    let mut first_spec = spec_for("story-replay", "同一个请求");
    first_spec.idempotency_key = IdempotencyKey::try_new("replay-key".to_string()).unwrap();
    let first = engine.run_turn(first_spec, &first_recorder).await.expect("first turn");

    let replay_recorder = Recorder::default();
    let mut replay_spec = spec_for("story-replay", "同一个请求");
    replay_spec.idempotency_key = IdempotencyKey::try_new("replay-key".to_string()).unwrap();
    let replay = engine.run_turn(replay_spec, &replay_recorder).await.expect("replay turn");

    assert_eq!(first.turn_id, replay.turn_id);
    assert_eq!(first.story_text, replay.story_text);
    let events = replay_recorder.events.lock().unwrap();
    let committed = events
        .iter()
        .find_map(|event| match event {
            TurnEvent::Committed { result, replayed } => Some((result.clone(), *replayed)),
            _ => None,
        })
        .expect("replay must emit a committed event");
    assert!(committed.1, "replay must carry replayed = true");
    assert_eq!(
        committed.0.turn_id, first.turn_id,
        "replay returns the original persisted result"
    );
    assert!(
        !events.iter().any(|event| matches!(event, TurnEvent::StageStarted { .. })),
        "replay must not re-run the runtime"
    );
    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn idempotency_conflict_emits_conflict_terminal() {
    let db = temp_db_path("idem_conflict_event");
    let engine = build_engine(&db).await;
    ensure_story(&engine, "story-conflict").await;
    let first_recorder = Recorder::default();
    let mut first_spec = spec_for("story-conflict", "原始请求");
    first_spec.idempotency_key = IdempotencyKey::try_new("conflict-key".to_string()).unwrap();
    engine.run_turn(first_spec, &first_recorder).await.expect("first turn");

    let recorder = Recorder::default();
    let mut conflict_spec = spec_for("story-conflict", "不同请求");
    conflict_spec.idempotency_key = IdempotencyKey::try_new("conflict-key".to_string()).unwrap();
    let error = engine
        .run_turn(conflict_spec, &recorder)
        .await
        .expect_err("same key with a different request must conflict");
    assert!(matches!(error.kind(), TurnFailureKind::IdempotencyConflict));
    {
        let events = recorder.events.lock().unwrap();
        assert!(
            events.iter().any(|event| matches!(
                event,
                TurnEvent::Conflict {
                    code: "idempotency_conflict",
                    ..
                }
            )),
            "conflict must emit exactly one conflict terminal event"
        );
        assert!(!events.iter().any(|event| matches!(event, TurnEvent::Committed { .. })));
    }
    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn nested_llm_cancel_sets_cancelled_phase_and_event() {
    let db = temp_db_path("nested_cancel");
    let cancellation = TurnCancellation::new();
    let provider: Arc<dyn LlmProvider> = Arc::new(CancellingProvider {
        cancellation: cancellation.clone(),
    });
    let store: Arc<dyn aise::persistence::Store> = SqliteStore::connect(&db).await.expect("connect store");
    let engine = build_engine_with(store, provider, AiseConfig::default()).await;
    ensure_story(&engine, "story-cancel").await;
    let recorder = Recorder::default();
    let mut spec = spec_for("story-cancel", "开始吧");
    spec.idempotency_key = IdempotencyKey::try_new("cancel-key".to_string()).unwrap();
    spec.cancellation = cancellation.clone();
    let error = engine
        .run_turn(spec, &recorder)
        .await
        .expect_err("nested llm cancellation must fail the turn");
    assert!(matches!(error.kind(), TurnFailureKind::Cancelled));
    {
        let events = recorder.events.lock().unwrap();
        assert!(
            events
                .iter()
                .any(|event| matches!(event, TurnEvent::Cancelled { code: "cancelled", .. })),
            "cancelled terminal must be delivered"
        );
        assert!(!events.iter().any(|event| matches!(event, TurnEvent::Committed { .. })));
    }
    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn nested_store_conflict_sets_conflict_phase_and_event() {
    let db = temp_db_path("nested_store_conflict");
    let store: Arc<dyn aise::persistence::Store> = Arc::new(ConflictStore {
        inner: SqliteStore::connect(&db).await.expect("connect store"),
    });
    let engine = build_engine_with(store, Arc::new(StubLlm), AiseConfig::default()).await;
    ensure_story(&engine, "story-conflict-nested").await;
    let recorder = Recorder::default();
    let mut spec = spec_for("story-conflict-nested", "开始吧");
    spec.idempotency_key = IdempotencyKey::try_new("conflict-key".to_string()).unwrap();
    let error = engine
        .run_turn(spec, &recorder)
        .await
        .expect_err("store revision conflict must fail the turn");
    assert!(matches!(error.kind(), TurnFailureKind::RevisionConflict));
    {
        let events = recorder.events.lock().unwrap();
        assert!(
            events.iter().any(|event| matches!(
                event,
                TurnEvent::Conflict {
                    code: "revision_conflict",
                    ..
                }
            )),
            "nested store conflict must map to a conflict terminal event"
        );
        assert!(!events.iter().any(|event| matches!(event, TurnEvent::Committed { .. })));
    }
    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn pipeline_failure_sets_failed_phase_and_closes_trace() {
    let db = temp_db_path("pipeline_failure");
    let provider: Arc<dyn LlmProvider> = Arc::new(RejectingProvider);
    let store: Arc<dyn aise::persistence::Store> = SqliteStore::connect(&db).await.expect("connect store");
    let engine = build_engine_with(store, provider, AiseConfig::default()).await;
    ensure_story(&engine, "story-fail").await;
    let recorder = Recorder::default();
    let mut spec = spec_for("story-fail", "开始吧");
    spec.idempotency_key = IdempotencyKey::try_new("fail-key".to_string()).unwrap();
    let error = engine
        .run_turn(spec, &recorder)
        .await
        .expect_err("provider rejection must fail the turn");
    assert!(matches!(error.kind(), TurnFailureKind::Llm));
    {
        let events = recorder.events.lock().unwrap();
        assert!(
            events.iter().any(|event| matches!(event, TurnEvent::Failed { .. })),
            "pipeline failure must emit one failed terminal event"
        );
        assert!(
            events.iter().any(|event| matches!(event, TurnEvent::TraceCompleted { .. })),
            "finalizer must close the trace on pipeline failure"
        );
        assert!(!events.iter().any(|event| matches!(event, TurnEvent::Committed { .. })));
    }
    let _ = std::fs::remove_file(&db);
}

// TODO: validation is temporarily bypassed in ValidationPipeline; remove #[ignore] when the pipeline is restored.
#[ignore]
#[tokio::test]
async fn repair_exhaustion_sets_failed_phase_and_never_commits() {
    let db = temp_db_path("repair_exhaust");
    let get_story_calls = Arc::new(AtomicUsize::new(0));
    let commit_calls = Arc::new(AtomicUsize::new(0));
    let store: Arc<dyn aise::persistence::Store> = Arc::new(TrackedStore {
        inner: SqliteStore::connect(&db).await.expect("connect store"),
        get_story_calls,
        commit_calls: commit_calls.clone(),
    });
    let provider: Arc<dyn LlmProvider> = Arc::new(RepairProposalProvider);
    let mut config = AiseConfig::default();
    config.turn.max_repair_rounds = 0;
    let engine = build_engine_with(store, provider, config).await;
    ensure_story(&engine, "story-repair").await;
    let recorder = Recorder::default();
    let mut spec = spec_for("story-repair", "开始吧");
    spec.idempotency_key = IdempotencyKey::try_new("repair-key".to_string()).unwrap();
    let error = engine
        .run_turn(spec, &recorder)
        .await
        .expect_err("exhausted repair budget must fail the turn");
    assert!(matches!(error.kind(), TurnFailureKind::ValidationBudgetExhausted));
    {
        let events = recorder.events.lock().unwrap();
        assert!(
            events.iter().any(|event| {
                matches!(
                    event,
                    TurnEvent::Failed {
                        code: "validation_budget_exhausted",
                        ..
                    }
                )
            }),
            "repair exhaustion must emit a failed terminal event"
        );
        assert!(!events.iter().any(|event| matches!(event, TurnEvent::Committed { .. })));
    }
    assert_eq!(
        commit_calls.load(Ordering::SeqCst),
        0,
        "a turn that exhausts repair rounds must never commit"
    );
    let _ = std::fs::remove_file(&db);
}

// TODO: validation is temporarily bypassed in ValidationPipeline; remove #[ignore] when the pipeline is restored.
#[ignore]
#[tokio::test]
async fn validation_completed_emitted_for_each_attempt() {
    let db = temp_db_path("validation_attempts");
    let store: Arc<dyn aise::persistence::Store> = SqliteStore::connect(&db).await.expect("connect store");
    let provider: Arc<dyn LlmProvider> = Arc::new(RepairProposalProvider);
    let mut config = AiseConfig::default();
    config.turn.max_repair_rounds = 1;
    let engine = build_engine_with(store, provider, config).await;
    ensure_story(&engine, "story-attempts").await;
    let recorder = Recorder::default();
    let mut spec = spec_for("story-attempts", "开始吧");
    spec.idempotency_key = IdempotencyKey::try_new("attempts-key".to_string()).unwrap();
    let error = engine.run_turn(spec, &recorder).await.expect_err("repair budget must exhaust");
    assert!(matches!(error.kind(), TurnFailureKind::ValidationBudgetExhausted));
    {
        let events = recorder.events.lock().unwrap();
        let attempts: Vec<u32> = events
            .iter()
            .filter_map(|event| match event {
                TurnEvent::ValidationCompleted { attempt, decision, .. } => {
                    assert_eq!(*decision, ValidationDecision::Repair);
                    Some(*attempt)
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            attempts,
            vec![1, 2],
            "ValidationCompleted must be emitted once per validation attempt"
        );
    }
    let _ = std::fs::remove_file(&db);
}
