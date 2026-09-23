use crate::api::dto::TurnRequest;
use crate::api::sse::{ClientDisconnectGuard, SSE_CHANNEL_CAPACITY, SseSink};
use crate::api::state::AppState;
use crate::error::ApiError;
use crate::turn_submission::{TurnSubmissionError, TurnSubmissionRequest};
use aise::domain::ids::StoryId;
use aise::turn::turn_contract::{IdempotencyKey, TurnCancellation};
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
    let include_trace = req.include_trace;
    let raw_idempotency_key = idempotency_key_header(&headers)?;

    let (progress_tx, progress_rx) = mpsc::channel(SSE_CHANNEL_CAPACITY);
    let (terminal_tx, terminal_rx) = mpsc::channel(1);
    let sink = Arc::new(SseSink::new(progress_tx, terminal_tx, include_trace));

    let cancellation = TurnCancellation::new();
    let submission = TurnSubmissionRequest {
        raw_session_id: id,
        raw_idempotency_key,
        player_contribution: req.player_contribution,
        cancellation: cancellation.clone(),
    };
    state
        .turn_submission
        .submit(submission, sink)
        .await
        .map_err(submission_api_error)?;

    let stream = crate::api::sse::sse_merged_stream(progress_rx, terminal_rx, ClientDisconnectGuard::new(cancellation));
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

fn submission_api_error(error: TurnSubmissionError) -> ApiError {
    match error {
        TurnSubmissionError::InvalidSession => ApiError::BadRequest("invalid session id".into()),
        TurnSubmissionError::SessionNotFound => ApiError::NotFound("session".into()),
        TurnSubmissionError::InvalidRequest(message) => ApiError::BadRequest(message),
        TurnSubmissionError::MissingIdempotencyKey => ApiError::BadRequest("missing Idempotency-Key header".into()),
        TurnSubmissionError::InvalidIdempotencyKey(message) => ApiError::BadRequest(message),
        TurnSubmissionError::Admission(message) => ApiError::Backpressure(message),
    }
}

fn idempotency_key_header(headers: &HeaderMap) -> Result<Option<String>, ApiError> {
    headers
        .get("Idempotency-Key")
        .map(|value| {
            value
                .to_str()
                .map(str::to_owned)
                .map_err(|_| ApiError::BadRequest("invalid Idempotency-Key header".into()))
        })
        .transpose()
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

#[cfg(test)]
#[path = "tests/turn_tests.rs"]
mod tests;
