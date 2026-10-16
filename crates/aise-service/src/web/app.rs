use super::config::WebConfig;
use super::{page, sink};
use crate::observability::{DETECTOR_OVERLAP_BYTES, ObservabilityConfig};
use aise_core::core::{
    CharacterCard, IdempotencyKey, PlayerId, RoleId, StoryCommit, StoryId, StoryPack, TurnCancellation, TurnControl,
    TurnRequest,
};
use aise_core::engine::{AiseEngine, Engine, EngineError, RoleActorSpec, StoryCreationSpec};
use aise_core::trace::{
    Attribute, ContentCapture, ObservabilityContentConfig, ObservationError, ObservationSession, ObservationStatus,
    SessionOutcome, SessionSpec, TRACE_ENVIRONMENT, TRACE_METADATA_STORY_ID, TRACE_METADATA_TURN_NUMBER, TRACE_RELEASE,
    Trace, TraceOutcome, TraceSpec,
};
use anyhow::Context;
use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::{KeepAlive, Sse};
use axum::routing::{get, post};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::sync::{Semaphore, mpsc};
use uuid::Uuid;

const DEMO_PLAYER_ID: &str = "web-player";
const DEMO_PLAYER_ROLE: &str = "xu_xian";

struct AppState {
    engine: Arc<AiseEngine>,
    story_id: StoryId,
    story_title: String,
    opening: String,
    turn_timeout: Duration,
    max_input_bytes: usize,
    event_buffer_capacity: usize,
    turn_gate: Arc<Semaphore>,
    trace_context: WebTraceContext,
}

#[derive(Clone)]
struct WebTraceContext {
    session_id: String,
    content: ContentCapture,
    environment: String,
    release: String,
}

#[derive(Serialize)]
struct StoryView {
    title: String,
    opening: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TurnPayload {
    message: String,
}

pub async fn run(config: WebConfig, observability_config: &ObservabilityConfig) -> anyhow::Result<()> {
    let bind_address = config.bind_address;
    let state = Arc::new(build_state(config, observability_config).await?);
    let app = Router::new()
        .route("/", get(page::index))
        .route("/api/story", get(get_story))
        .route("/api/turn", post(run_turn))
        .with_state(state);
    let listener = TcpListener::bind(bind_address)
        .await
        .with_context(|| format!("failed to bind web service to {bind_address}"))?;
    tracing::info!(address = %bind_address, "aise web service started");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("web service failed")
}

async fn build_state(config: WebConfig, observability_config: &ObservabilityConfig) -> anyhow::Result<AppState> {
    let story_pack: StoryPack = serde_json::from_str(
        &tokio::fs::read_to_string(&config.story_pack_path)
            .await
            .with_context(|| format!("failed to read story pack {}", config.story_pack_path.display()))?,
    )
    .context("story pack parse failed")?;
    let character_card: CharacterCard = serde_json::from_str(
        &tokio::fs::read_to_string(&config.character_card_path)
            .await
            .with_context(|| format!("failed to read character card {}", config.character_card_path.display()))?,
    )
    .context("character card parse failed")?;
    let story_title = story_pack.profile.title.as_str().to_owned();
    let opening = story_pack.opening.as_str().to_owned();
    let engine = Arc::new(AiseEngine::new(config.engine).context("engine initialization failed")?);
    let story = engine
        .create_story(story_creation_spec(story_pack, character_card)?)
        .await
        .context("story creation failed")?;
    Ok(AppState {
        engine,
        story_id: story.story_id,
        story_title,
        opening,
        turn_timeout: config.turn_timeout,
        max_input_bytes: config.max_input_bytes,
        event_buffer_capacity: config.event_buffer_capacity.get(),
        turn_gate: Arc::new(Semaphore::new(1)),
        trace_context: WebTraceContext {
            session_id: Uuid::new_v4().to_string(),
            content: ContentCapture::new(ObservabilityContentConfig {
                policy: observability_config.content_policy.clone(),
                max_field_bytes: observability_config.max_field_bytes,
                max_observation_bytes: observability_config.max_observation_bytes,
                detector_overlap_bytes: DETECTOR_OVERLAP_BYTES,
            }),
            environment: observability_config.environment.clone(),
            release: observability_config.release.clone(),
        },
    })
}

async fn get_story(State(state): State<Arc<AppState>>) -> Json<StoryView> {
    Json(StoryView {
        title: state.story_title.clone(),
        opening: state.opening.clone(),
    })
}

async fn run_turn(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<TurnPayload>,
) -> Result<Sse<impl futures::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>>, ApiError> {
    validate_player_input(&payload.message, state.max_input_bytes)?;
    let permit = state
        .turn_gate
        .clone()
        .try_acquire_owned()
        .map_err(|_| (StatusCode::CONFLICT, "已有回合正在执行，请稍后再试".to_owned()))?;
    let idempotency_key = IdempotencyKey::try_new(format!("web-turn-{}", Uuid::new_v4()))
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    let request = TurnRequest {
        story_id: state.story_id.clone(),
        idempotency_key,
        player_input: payload.message,
    };
    let cancellation = TurnCancellation::new();
    let control = TurnControl::new(Instant::now() + state.turn_timeout, cancellation.clone());
    let (sender, receiver) = mpsc::channel(state.event_buffer_capacity);
    let event_sink = sink::WebTurnEventSink::new(sender.clone(), cancellation.clone());
    let engine = Arc::clone(&state.engine);
    let story_id = state.story_id.clone();
    let trace_context = state.trace_context.clone();
    tokio::spawn(async move {
        let _permit = permit;
        let (session, mut trace) = begin_turn_trace(&trace_context, &story_id, &request.player_input);
        let result = engine.run_turn(request, &control, &event_sink, &trace).await;
        let status = result_status(&result);
        if let Ok(commit) = &result {
            trace.bind(vec![Attribute::u64(
                TRACE_METADATA_TURN_NUMBER,
                commit.turn.turn_number.value(),
            )]);
        }
        let outcome = trace_outcome(&trace, &result);
        trace.finish(outcome);
        session.finish(SessionOutcome {
            status,
            metadata: Vec::new(),
        });
        let terminal_event = match result {
            Ok(commit) => sink::committed_event(commit.turn.turn_segment.text(), commit.turn.turn_number.value()),
            Err(error) => {
                tracing::error!(story_id = %story_id, error = %error, "web turn failed");
                sink::error_event(&error.to_string())
            }
        };
        if sender.send(terminal_event).await.is_err() {
            tracing::warn!(story_id = %story_id, error_kind = "client_disconnected", "web turn result receiver closed");
        }
    });
    Ok(Sse::new(sink::event_stream(receiver, cancellation)).keep_alive(KeepAlive::default()))
}

fn validate_player_input(message: &str, max_input_bytes: usize) -> Result<(), ApiError> {
    if message.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "消息不能为空".to_owned()));
    }
    if message.len() > max_input_bytes {
        return Err((StatusCode::PAYLOAD_TOO_LARGE, format!("消息不能超过 {max_input_bytes} 字节")));
    }
    Ok(())
}

fn story_creation_spec(story_pack: StoryPack, card: CharacterCard) -> anyhow::Result<StoryCreationSpec> {
    let player_role = RoleId::try_new(DEMO_PLAYER_ROLE).context("invalid demo player role")?;
    Ok(StoryCreationSpec {
        story_pack,
        cast: BTreeMap::from([(player_role.clone(), RoleActorSpec::Card(Box::new(card)))]),
        player_id: PlayerId::try_new(DEMO_PLAYER_ID).context("invalid demo player id")?,
        player_role,
    })
}

fn begin_turn_trace(
    trace_context: &WebTraceContext,
    story_id: &StoryId,
    player_input: &str,
) -> (ObservationSession, Trace) {
    let session = ObservationSession::begin(
        SessionSpec {
            id: Some(trace_context.session_id.clone()),
            user_id: Some(DEMO_PLAYER_ID.to_owned()),
            metadata: Vec::new(),
        },
        trace_context.content.clone(),
    );
    let mut trace = session.begin_trace(TraceSpec {
        name: "web-turn",
        input: trace_context
            .content
            .encode(&player_input, trace_context.content.max_observation_bytes())
            .content,
        metadata: vec![
            Attribute::string(TRACE_ENVIRONMENT, trace_context.environment.clone()),
            Attribute::string(TRACE_RELEASE, trace_context.release.clone()),
        ],
        tags: vec!["story-turn".to_owned(), "web-turn".to_owned()],
    });
    trace.bind(vec![Attribute::string(TRACE_METADATA_STORY_ID, story_id.to_string())]);
    (session, trace)
}

fn trace_outcome(trace: &Trace, result: &Result<StoryCommit, EngineError>) -> TraceOutcome {
    match result {
        Ok(commit) => TraceOutcome {
            status: ObservationStatus::Ok,
            output: trace
                .content_capture()
                .encode(
                    &commit.turn.turn_segment.text().to_owned(),
                    trace.content_capture().max_observation_bytes(),
                )
                .content,
            ..TraceOutcome::default()
        },
        Err(error) => TraceOutcome {
            status: ObservationStatus::Error,
            error: Some(ObservationError {
                code: "engine_error".to_owned(),
                failure_kind: "engine".to_owned(),
                stage: None,
                message: error.to_string(),
            }),
            ..TraceOutcome::default()
        },
    }
}

fn result_status(result: &Result<StoryCommit, EngineError>) -> ObservationStatus {
    if result.is_ok() {
        ObservationStatus::Ok
    } else {
        ObservationStatus::Error
    }
}

async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::error!(error = %error, "failed to listen for shutdown signal");
    }
}

type ApiError = (StatusCode, String);

#[cfg(test)]
#[path = "tests/app_tests.rs"]
mod tests;
