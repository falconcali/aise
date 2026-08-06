use aise::AiseConfig;
use aise::AiseEngine;
use aise::character::CharacterThinkPipeline;
use aise::context::{BaselineContextBuilder, ContextRetrievalPipeline};
use aise::engine::{SystemClock, UuidIdGenerator};
use aise::llm::LlmGateway;
use aise::llm::accounting::{FinishReason, LlmCompletion};
use aise::llm::error::{LlmProtocolErrorKind, LlmProviderError};
use aise::llm::message::{CompletionRequest, EmbeddingOutput, EmbeddingRequest};
use aise::llm::provider::{DeltaSink, LlmProvider};
use aise::persistence::{SqliteStore, TurnCommitter};
use aise::planning::WriterPlanner;
use aise::runtime::{StoryTurnCoordinator, TurnInitializer, TurnPipelineSet, TurnRuntime};
use aise::story::{StoryGenerator, StoryRepairer};
use aise::validation::ValidationPipeline;
use aise_server::session::SessionRegistry;
use aise_server::tasks::{TurnTaskSupervisor, TurnTaskSupervisorConfig};
use aise_server::{AppState, ServerConfig, router};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

struct StubProvider;

#[async_trait::async_trait]
impl LlmProvider for StubProvider {
    fn provider_name(&self) -> &'static str {
        "stub"
    }

    async fn complete(&self, _req: &CompletionRequest) -> Result<LlmCompletion, LlmProviderError> {
        Ok(LlmCompletion {
            text: "Hello World".to_string(),
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

async fn build_engine(db_url: &str) -> Arc<AiseEngine> {
    let store: Arc<dyn aise::persistence::Store> = SqliteStore::connect(db_url).await.expect("connect store");
    let provider: Arc<dyn LlmProvider> = Arc::new(StubProvider);
    let config = AiseConfig::default();
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

fn temp_db_path(label: &str) -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir()
        .join(format!("aise_server_story_{label}_{now}.db"))
        .to_string_lossy()
        .into_owned()
}

async fn create_story_via_api(app: &axum::Router, instructions: &str) -> String {
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/api/stories")
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(
            serde_json::json!({ "story_instructions": instructions }).to_string(),
        ))
        .unwrap();
    let response = tower::ServiceExt::oneshot(app.clone(), request).await.expect("router call");
    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    let body = response.into_body();
    let bytes = axum::body::to_bytes(body, 1024 * 1024).await.expect("read story body");
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("parse story json");
    json["story_id"].as_str().expect("story id").to_string()
}

#[tokio::test]
async fn multiple_sessions_can_bind_same_story() {
    let db = temp_db_path("multi_bind");
    let engine = build_engine(&db).await;
    let registry = SessionRegistry::new(8);
    let tasks = TurnTaskSupervisor::new(TurnTaskSupervisorConfig::default()).unwrap();
    let config = ServerConfig::default();
    let state = Arc::new(AppState::new(engine, registry, tasks.clone(), config.clone()));
    let app = router(state, &config);

    let story_id = create_story_via_api(&app, "multi-bind story").await;

    let session_one = axum::http::Request::builder()
        .method("POST")
        .uri("/api/sessions")
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(
            serde_json::json!({ "name": "s1", "story_id": story_id }).to_string(),
        ))
        .unwrap();
    let response = tower::ServiceExt::oneshot(app.clone(), session_one).await.expect("router call");
    assert_eq!(response.status(), axum::http::StatusCode::CREATED);

    let session_two = axum::http::Request::builder()
        .method("POST")
        .uri("/api/sessions")
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(
            serde_json::json!({ "name": "s2", "story_id": story_id }).to_string(),
        ))
        .unwrap();
    let response = tower::ServiceExt::oneshot(app.clone(), session_two).await.expect("router call");
    assert_eq!(response.status(), axum::http::StatusCode::CREATED);

    let list = axum::http::Request::builder()
        .method("GET")
        .uri("/api/sessions")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = tower::ServiceExt::oneshot(app, list).await.expect("router call");
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read sessions");
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("parse sessions");
    assert_eq!(json.as_array().expect("array").len(), 2, "two sessions coexist on one story");

    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn session_binds_existing_story_after_restart() {
    let db = temp_db_path("restart");
    let engine = build_engine(&db).await;
    let tasks = TurnTaskSupervisor::new(TurnTaskSupervisorConfig::default()).unwrap();
    let config = ServerConfig::default();

    let story_id = {
        let registry = SessionRegistry::new(4);
        let state = Arc::new(AppState::new(engine.clone(), registry, tasks.clone(), config.clone()));
        let app = router(state, &config);
        create_story_via_api(&app, "persistent story").await
    };

    let fresh_registry = SessionRegistry::new(4);
    let state = Arc::new(AppState::new(engine, fresh_registry, tasks, config.clone()));
    let app = router(state, &config);

    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/api/sessions")
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(
            serde_json::json!({ "name": "after-restart", "story_id": story_id }).to_string(),
        ))
        .unwrap();
    let response = tower::ServiceExt::oneshot(app.clone(), request).await.expect("router call");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::CREATED,
        "a fresh session registry locates the persistent story"
    );

    let get_story = axum::http::Request::builder()
        .method("GET")
        .uri(format!("/api/stories/{story_id}"))
        .body(axum::body::Body::empty())
        .unwrap();
    let response = tower::ServiceExt::oneshot(app, get_story).await.expect("router call");
    assert_eq!(response.status(), axum::http::StatusCode::OK, "story survives session restart");

    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn create_session_requires_existing_story() {
    let db = temp_db_path("no_auto_create");
    let engine = build_engine(&db).await;
    let registry = SessionRegistry::new(4);
    let tasks = TurnTaskSupervisor::new(TurnTaskSupervisorConfig::default()).unwrap();
    let config = ServerConfig::default();
    let state = Arc::new(AppState::new(engine.clone(), registry, tasks.clone(), config.clone()));
    let app = router(state, &config);

    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/api/sessions")
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(
            serde_json::json!({ "name": "ghost", "story_id": "story-that-does-not-exist" }).to_string(),
        ))
        .unwrap();
    let response = tower::ServiceExt::oneshot(app.clone(), request).await.expect("router call");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::NOT_FOUND,
        "session creation must not auto-create a story"
    );
    assert!(
        engine
            .store()
            .get_story(&aise::domain::StoryId::try_new("story-that-does-not-exist").unwrap())
            .await
            .expect("get story")
            .is_none(),
        "no story row may be created implicitly"
    );

    let bind = axum::http::Request::builder()
        .method("PUT")
        .uri("/api/sessions/missing-session/story")
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(
            serde_json::json!({ "story_id": "story-that-does-not-exist" }).to_string(),
        ))
        .unwrap();
    let response = tower::ServiceExt::oneshot(app, bind).await.expect("router call");
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);

    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn turn_result_recovery_does_not_auto_create_story() {
    let db = temp_db_path("recover_404");
    let engine = build_engine(&db).await;
    let registry = SessionRegistry::new(4);
    let tasks = TurnTaskSupervisor::new(TurnTaskSupervisorConfig::default()).unwrap();
    let config = ServerConfig::default();
    let state = Arc::new(AppState::new(engine, registry, tasks, config.clone()));
    let app = router(state, &config);

    let request = axum::http::Request::builder()
        .method("GET")
        .uri("/api/stories/ghost-story/turn-results/ghost-key")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = tower::ServiceExt::oneshot(app, request).await.expect("router call");
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);

    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn deleting_session_does_not_delete_story() {
    let db = temp_db_path("session_delete");
    let engine = build_engine(&db).await;
    let registry = SessionRegistry::new(4);
    let tasks = TurnTaskSupervisor::new(TurnTaskSupervisorConfig::default()).unwrap();
    let config = ServerConfig::default();
    let state = Arc::new(AppState::new(engine.clone(), registry, tasks.clone(), config.clone()));
    let app = router(state, &config);

    let story_id = create_story_via_api(&app, "session-scoped story").await;

    let create_session = axum::http::Request::builder()
        .method("POST")
        .uri("/api/sessions")
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(
            serde_json::json!({ "name": "to-delete", "story_id": story_id }).to_string(),
        ))
        .unwrap();
    let response = tower::ServiceExt::oneshot(app.clone(), create_session)
        .await
        .expect("router call");
    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    let body = response.into_body();
    let bytes = axum::body::to_bytes(body, 1024 * 1024).await.expect("read session body");
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("parse session json");
    let session_id = json["id"].as_str().expect("session id").to_string();

    let delete_session = axum::http::Request::builder()
        .method("DELETE")
        .uri(format!("/api/sessions/{session_id}"))
        .body(axum::body::Body::empty())
        .unwrap();
    let response = tower::ServiceExt::oneshot(app.clone(), delete_session)
        .await
        .expect("router call");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::NO_CONTENT,
        "deleting a session succeeds"
    );

    let get_story = axum::http::Request::builder()
        .method("GET")
        .uri(format!("/api/stories/{story_id}"))
        .body(axum::body::Body::empty())
        .unwrap();
    let response = tower::ServiceExt::oneshot(app.clone(), get_story).await.expect("router call");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "deleting a session must not delete its bound story"
    );

    let list = axum::http::Request::builder()
        .method("GET")
        .uri("/api/sessions")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = tower::ServiceExt::oneshot(app, list).await.expect("router call");
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read sessions");
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("parse sessions");
    assert_eq!(json.as_array().expect("array").len(), 0, "the session itself is gone");

    let _ = std::fs::remove_file(&db);
}
