use crate::api::state::AppState;
use crate::error::ApiError;
use crate::session::{SessionId, SessionInfo};
use aise::domain::StoryId;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct BindStoryRequest {
    pub story_id: String,
}

pub async fn bind_story(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(req): Json<BindStoryRequest>,
) -> Result<(StatusCode, Json<SessionInfo>), ApiError> {
    let story_id = StoryId::try_new(req.story_id).map_err(|error| ApiError::BadRequest(error.to_string()))?;
    let story = state
        .engine
        .store()
        .get_story(&story_id)
        .await
        .map_err(|error| ApiError::Internal(anyhow::anyhow!(error)))?;
    if story.is_none() {
        return Err(ApiError::NotFound("story".into()));
    }
    let session_id = SessionId::new(session_id);
    if !state.registry.bind_story(&session_id, story_id).await {
        return Err(ApiError::NotFound("session".into()));
    }
    let session = state
        .registry
        .get(&session_id)
        .await
        .ok_or_else(|| ApiError::NotFound("session".into()))?;
    Ok((StatusCode::OK, Json(session.info())))
}
