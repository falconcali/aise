use crate::api::state::AppState;
use crate::error::ApiError;
use aise::core::turn_data::SnapshotLimits;
use aise::domain::ids::StoryId;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct CreateStoryRequest {
    pub story_instructions: Option<String>,
    #[serde(default)]
    pub style: Option<String>,
    #[serde(default)]
    pub point_of_view: Option<String>,
    #[serde(default)]
    pub tense: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StoryView {
    pub story_id: String,
    pub base_revision: u64,
    pub story_instructions: String,
    pub current_scene: String,
    pub recent_story: Vec<String>,
}

pub async fn create_story(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateStoryRequest>,
) -> Result<(StatusCode, Json<StoryView>), ApiError> {
    let story_id =
        StoryId::try_new(uuid::Uuid::new_v4().to_string()).map_err(|error| ApiError::BadRequest(error.to_string()))?;
    let spec = aise::domain::StoryCreateSpec {
        story_id: story_id.clone(),
        story_instructions: req.story_instructions.unwrap_or_default(),
        story_config: aise::domain::StoryConfig {
            style: req.style,
            point_of_view: req.point_of_view,
            tense: req.tense,
        },
        player_character_id: None,
        initial_world: None,
        current_scene: aise::domain::CurrentScene { text: String::new() },
        story_summary: aise::domain::StorySummary { text: String::new() },
        active_constraints: Vec::new(),
        created_at_ms: now_millis(),
    };
    let info = state
        .engine
        .store()
        .create_story(&spec)
        .await
        .map_err(|error| ApiError::Internal(anyhow::anyhow!(error)))?;
    let view = StoryView {
        story_id: info.story_id.to_string(),
        base_revision: info.base_revision.get(),
        story_instructions: spec.story_instructions,
        current_scene: String::new(),
        recent_story: Vec::new(),
    };
    Ok((StatusCode::CREATED, Json(view)))
}

pub async fn get_story(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<StoryView>, ApiError> {
    let story_id = StoryId::try_new(id).map_err(|error| ApiError::BadRequest(error.to_string()))?;
    let limits = SnapshotLimits::from_config(&state.config.aise.content);
    let snapshot = state
        .engine
        .store()
        .load_story_snapshot(&story_id, limits)
        .await
        .map_err(|error| match error {
            aise::persistence::StoreError::NotFound => ApiError::NotFound("story".into()),
            other => ApiError::Internal(anyhow::anyhow!(other)),
        })?;
    Ok(Json(StoryView {
        story_id: snapshot.story_id().to_string(),
        base_revision: snapshot.base_revision().get(),
        story_instructions: snapshot.story_instructions().to_owned(),
        current_scene: snapshot.current_scene().text.clone(),
        recent_story: snapshot.recent_turns().iter().map(|turn| turn.story_text.clone()).collect(),
    }))
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}
