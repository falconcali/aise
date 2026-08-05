use crate::api::dto::TurnRequest;
use crate::api::sse::{ClientDisconnectGuard, SSE_CHANNEL_CAPACITY, SseSink, sse_stream};
use crate::api::state::AppState;
use crate::error::ApiError;
use crate::session::SessionId;
use aise::ExecuteTurnSpec;
use aise::core::turn_contract::{IdempotencyKey, TurnCancellation};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::Stream;
use std::convert::Infallible;
use std::sync::Arc;

pub async fn run_turn(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(req): Json<TurnRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let session = state
        .registry
        .get(&SessionId::new(id))
        .await
        .ok_or_else(|| ApiError::NotFound("session".into()))?;
    let story_id = session.story_id.clone();
    let player_input = req.player_input;
    let include_trace = req.include_trace;

    let (tx, rx) = futures::channel::mpsc::channel(SSE_CHANNEL_CAPACITY);
    let sink = SseSink::new(tx, include_trace);

    let idempotency_key = headers
        .get("Idempotency-Key")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .ok_or_else(|| ApiError::BadRequest("missing Idempotency-Key header".into()))
        .and_then(|value| IdempotencyKey::try_new(value).map_err(|e| ApiError::BadRequest(e.to_string())))?;
    let cancellation = TurnCancellation::new();
    let spec = ExecuteTurnSpec {
        story_id,
        idempotency_key,
        player_input,
        cancellation: cancellation.clone(),
    };

    let engine = state.engine.clone();
    state
        .tasks
        .spawn(async move {
            let result = engine.run_turn(spec, &sink).await;
            if let Err(error) = result {
                tracing::error!(%error, "turn task failed");
            }
        })
        .await
        .map_err(|e| ApiError::Backpressure(e.to_string()))?;

    let stream = sse_stream(rx, ClientDisconnectGuard::new(cancellation));
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
