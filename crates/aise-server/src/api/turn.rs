use crate::api::dto::TurnRequest;
use crate::api::sse::{ClientDisconnectGuard, SSE_CHANNEL_CAPACITY, SseSink};
use crate::api::state::AppState;
use crate::error::ApiError;
use crate::session::SessionId;
use aise::ExecuteTurnSpec;
use aise::core::turn_contract::{IdempotencyKey, TurnCancellation};
use aise::domain::ids::StoryId;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::Stream;
use std::convert::Infallible;
use std::sync::Arc;
use tokio::sync::mpsc;

pub async fn run_turn(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(req): Json<TurnRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let session_id = SessionId::try_new(id).map_err(|_| ApiError::BadRequest("invalid session id".into()))?;
    let session = state
        .registry
        .get(&session_id)
        .await
        .ok_or_else(|| ApiError::NotFound("session".into()))?;
    let story_id = session.story_id.clone();
    let include_trace = req.include_trace;

    aise::core::turn_contract::TurnRequest::try_new(req.player_input.clone())
        .map_err(|error| ApiError::BadRequest(error.to_string()))?;

    let idempotency_key = headers
        .get("Idempotency-Key")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .ok_or_else(|| ApiError::BadRequest("missing Idempotency-Key header".into()))
        .and_then(|value| IdempotencyKey::try_new(value).map_err(|e| ApiError::BadRequest(e.to_string())))?;

    let (progress_tx, progress_rx) = mpsc::channel(SSE_CHANNEL_CAPACITY);
    let (terminal_tx, terminal_rx) = mpsc::channel(1);
    let sink = SseSink::new(progress_tx, terminal_tx, include_trace);

    let cancellation = TurnCancellation::new();
    let spec = ExecuteTurnSpec {
        story_id,
        idempotency_key,
        player_input: req.player_input,
        cancellation: cancellation.clone(),
    };

    let engine = state.engine.clone();
    let task = crate::tasks::TurnTaskSpec {
        cancellation: cancellation.clone(),
        future: Box::pin(async move {
            let result = engine.run_turn(spec, &sink).await;
            if let Err(error) = result {
                tracing::error!(%error, "turn task failed");
            }
        }),
    };
    state
        .tasks
        .spawn(task)
        .await
        .map_err(|e| ApiError::Backpressure(e.to_string()))?;

    let stream = crate::api::sse::sse_merged_stream(progress_rx, terminal_rx, ClientDisconnectGuard::new(cancellation));
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

pub async fn get_turn_result(
    State(state): State<Arc<AppState>>,
    Path((story_id, idempotency_key)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let story_id = StoryId::try_new(story_id).map_err(|error| ApiError::BadRequest(error.to_string()))?;
    let idempotency_key =
        IdempotencyKey::try_new(idempotency_key).map_err(|error| ApiError::BadRequest(error.to_string()))?;
    match state.engine.store().find_committed_turn(&story_id, &idempotency_key).await {
        Ok(Some(outcome)) => Ok(Json(serde_json::json!(outcome.result))),
        Ok(None) => Err(ApiError::NotFound("turn_result_not_found".into())),
        Err(aise::persistence::StoreError::NotFound) => Err(ApiError::NotFound("story".into())),
        Err(_) => Err(ApiError::Backpressure("store_unavailable".into())),
    }
}
