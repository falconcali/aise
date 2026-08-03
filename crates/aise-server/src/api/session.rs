use crate::api::dto::CreateSessionRequest;
use crate::api::state::AppState;
use crate::error::ApiError;
use crate::session::{SessionId, SessionInfo};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use std::sync::Arc;

pub async fn create_session(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<(StatusCode, Json<SessionInfo>), ApiError> {
    let session = state
        .registry
        .create(req.name)
        .await
        .map_err(|e| ApiError::Conflict(e.to_string()))?;
    Ok((StatusCode::CREATED, Json(session.info())))
}

pub async fn list_sessions(State(state): State<Arc<AppState>>) -> Json<Vec<SessionInfo>> {
    Json(state.registry.list().await)
}

pub async fn delete_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let session_id = SessionId::new(id);
    if state.registry.delete(&session_id).await {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound("session".into()))
    }
}
