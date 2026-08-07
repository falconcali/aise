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
    pub player_character_id: Option<String>,
    pub turns: Vec<TurnView>,
    pub player_role_key: Option<String>,
    pub characters: Vec<CharacterStateView>,
}

#[derive(Debug, Serialize)]
pub struct TurnView {
    pub player_input: String,
    pub story_text: String,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct CharacterStateView {
    pub character_id: String,
    pub role_key: String,
    pub location: String,
    pub goals: Vec<String>,
    pub attributes: Vec<AttributeView>,
}

#[derive(Debug, Serialize)]
pub struct AttributeView {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Serialize)]
pub struct StoryInstanceView {
    pub story_id: String,
    pub base_revision: u64,
    pub pack_id: String,
    pub player_role_key: String,
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
        player_character_id: None,
        turns: Vec::new(),
        player_role_key: None,
        characters: Vec::new(),
    };
    Ok((StatusCode::CREATED, Json(view)))
}

pub async fn create_story_instance(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateStoryInstanceRequest>,
) -> Result<(StatusCode, Json<StoryInstanceView>), ApiError> {
    let pack_id = PackId::try_new(req.pack_id.clone()).map_err(|error| ApiError::BadRequest(error.to_string()))?;
    let player_id = PlayerId::try_new(req.player_id).map_err(|error| ApiError::BadRequest(error.to_string()))?;
    let player_role_key =
        StoryRoleKey::try_new(req.player_role_key.clone()).map_err(|error| ApiError::BadRequest(error.to_string()))?;
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
            pack_id: req.pack_id.clone(),
            player_role_key: req.player_role_key.clone(),
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
    let instance_meta = state
        .engine
        .store()
        .load_story_instance_meta(&story_id)
        .await
        .map_err(|error| ApiError::Internal(anyhow::anyhow!(error)))?;
    let player_role_key = instance_meta
        .as_ref()
        .and_then(|meta| meta.bindings.values().find(|binding| binding.player_id.is_some()))
        .map(|binding| binding.role_key.to_string());
    let characters = instance_meta
        .map(|meta| {
            meta.characters
                .into_values()
                .map(|character| CharacterStateView {
                    character_id: character.character_id.to_string(),
                    role_key: character.role_key.to_string(),
                    location: character.location.to_string(),
                    goals: character.goals.iter().map(|goal| goal.to_string()).collect(),
                    attributes: character
                        .attributes
                        .into_iter()
                        .map(|(key, value)| AttributeView {
                            key: key.to_string(),
                            value: serde_json::to_string(&value).unwrap_or_default(),
                        })
                        .collect(),
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Json(StoryView {
        story_id: snapshot.story_id().to_string(),
        base_revision: snapshot.base_revision().get(),
        story_instructions: snapshot.story_instructions().to_owned(),
        current_scene: snapshot.current_scene().text.clone(),
        player_character_id: snapshot.player_character_id().map(|id| id.to_string()),
        turns: snapshot
            .recent_turns()
            .iter()
            .map(|turn| TurnView {
                player_input: turn.player_input.clone(),
                story_text: turn.story_text.clone(),
                created_at: turn.created_at,
            })
            .collect(),
        player_role_key,
        characters,
    }))
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}
