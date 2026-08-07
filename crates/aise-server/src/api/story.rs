use crate::api::state::AppState;
use crate::error::ApiError;
use aise::core::turn_data::SnapshotLimits;
use aise::domain::asset::frozen_ref::FrozenCharacterAssetRef;
use aise::domain::asset::ids::{PackId, PlayerId, StoryRoleKey};
use aise::domain::ids::StoryId;
use aise::story::instance_factory::{CreateStoryInstanceSpec, StoryInstantiationError};
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

#[derive(Debug, Deserialize)]
pub struct CreateStoryInstanceRequest {
    pub pack_id: String,
    pub player_id: String,
    pub player_role_key: String,
    #[serde(default)]
    pub player_character: Option<PlayerCharacterRef>,
}

#[derive(Debug, Deserialize)]
pub struct PlayerCharacterRef {
    pub character_key: String,
    pub version: String,
    pub digest: String,
}

#[derive(Debug, Serialize)]
pub struct StoryView {
    pub story_id: String,
    pub base_revision: u64,
    pub story_instructions: String,
    pub current_scene: String,
    pub recent_story: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct StoryInstanceView {
    pub story_id: String,
    pub base_revision: u64,
    pub pack_id: String,
    pub current_scene: String,
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

pub async fn create_story_instance(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateStoryInstanceRequest>,
) -> Result<(StatusCode, Json<StoryInstanceView>), ApiError> {
    let pack_id = PackId::try_new(req.pack_id).map_err(|error| ApiError::BadRequest(error.to_string()))?;
    let player_id = PlayerId::try_new(req.player_id).map_err(|error| ApiError::BadRequest(error.to_string()))?;
    let player_role_key =
        StoryRoleKey::try_new(req.player_role_key).map_err(|error| ApiError::BadRequest(error.to_string()))?;
    let player_character = req
        .player_character
        .map(|reference| {
            Ok::<_, ApiError>(FrozenCharacterAssetRef {
                character_key: aise::domain::asset::ids::CharacterAssetKey::try_new(reference.character_key)
                    .map_err(|error| ApiError::BadRequest(error.to_string()))?,
                version: aise::domain::asset::ids::SemanticVersion::try_new(reference.version)
                    .map_err(|error| ApiError::BadRequest(error.to_string()))?,
                digest: aise::domain::asset::ids::Sha256Digest::try_new(&reference.digest)
                    .map_err(|error| ApiError::BadRequest(error.to_string()))?,
            })
        })
        .transpose()?;
    let instance_factory = state
        .instance_factory
        .as_ref()
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("instance factory not initialized")))?;
    let spec = CreateStoryInstanceSpec {
        pack_id,
        player_id,
        player_role_key,
        player_character,
        created_at_ms: now_millis(),
    };
    let info = instance_factory.create(spec).await.map_err(|error| match error {
        StoryInstantiationError::PackNotFound => ApiError::NotFound("story pack".into()),
        StoryInstantiationError::RoleNotFound => ApiError::Unprocessable("story role was not found".into()),
        StoryInstantiationError::RoleNotPlayable => ApiError::Unprocessable("story role is not playable".into()),
        StoryInstantiationError::CharacterNotFound => ApiError::Unprocessable("character asset was not found".into()),
        StoryInstantiationError::LimitExceeded { limit } => {
            ApiError::Unprocessable(format!("story instantiation limit exceeded: {limit}"))
        }
        StoryInstantiationError::Store(store_error) => ApiError::Internal(anyhow::anyhow!(store_error)),
    })?;
    Ok((
        StatusCode::CREATED,
        Json(StoryInstanceView {
            story_id: info.story_id.to_string(),
            base_revision: info.base_revision.get(),
            pack_id: info.story_id.to_string(),
            current_scene: String::new(),
        }),
    ))
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
