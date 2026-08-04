use crate::api::dto::TurnRequest;
use crate::api::state::AppState;
use crate::error::ApiError;
use crate::session::SessionId;
use aise::core::turn_contract::{IdempotencyKey, TurnCancellation};
use aise::{ExecuteTurnSpec, TurnEvent, TurnEventSink};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::channel::mpsc::UnboundedSender;
use futures::stream::{Stream, StreamExt};
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

    let (tx, rx) = futures::channel::mpsc::unbounded();
    let sink = SseSink { tx, include_trace };

    let idempotency_key = headers
        .get("Idempotency-Key")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .ok_or_else(|| ApiError::BadRequest("missing Idempotency-Key header".into()))
        .and_then(|value| IdempotencyKey::try_new(value).map_err(|e| ApiError::BadRequest(e.to_string())))?;
    let spec = ExecuteTurnSpec {
        story_id,
        idempotency_key,
        player_input,
        cancellation: TurnCancellation::new(),
    };

    let engine = state.engine.clone();
    tokio::spawn(async move {
        match engine.run_turn(spec, &sink).await {
            Ok(_) => {}
            Err(e) => tracing::error!(error = %e, "turn failed"),
        }
    });

    let stream = rx.map(Ok::<_, Infallible>);
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

struct SseSink {
    tx: UnboundedSender<Event>,
    include_trace: bool,
}

impl TurnEventSink for SseSink {
    fn emit(&self, event: TurnEvent) {
        let sse = match event {
            TurnEvent::StageStarted(stage) => Event::default().event("stage").data(stage.as_str()),
            TurnEvent::Token(text) => Event::default().event("token").data(text),
            TurnEvent::Validation { pass } => {
                Event::default().event("validation").data(if pass { "pass" } else { "fail" })
            }
            TurnEvent::Finished { turn_id } => Event::default().event("done").data(turn_id.to_string()),
            TurnEvent::Trace(trace) => {
                if !self.include_trace {
                    return;
                }
                match serde_json::to_string(&trace) {
                    Ok(json) => Event::default().event("trace").data(json),
                    Err(_) => return,
                }
            }
        };
        let _ = self.tx.unbounded_send(sse);
    }
}
