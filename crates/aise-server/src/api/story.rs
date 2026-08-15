use crate::api::state::AppState;
use crate::error::ApiError;
use aise::domain::asset::frozen_ref::FrozenCharacterCardRef;
use aise::domain::asset::ids::{PackId, PlayerId};
use aise::domain::ids::{RoleId, StoryId};
use aise::domain::story_sequence::StorySequence;
use aise::domain::turn::SnapshotLimits;
use aise::persistence::{StoryHistoryQuery, StoryOpeningView, StoryTurnView};
use aise::story::instance_factory::{CreateStoryInstanceSpec, StoryInstantiationError};
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateStoryInstanceRequest {
    pub pack_id: String,
    pub player_id: String,
    pub player_role_id: String,
    #[serde(default)]
    pub role_profiles: Vec<RoleProfileSelectionRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleProfileSelectionRequest {
    pub role_id: String,
    pub character_id: String,
    pub version: String,
    pub digest: String,
}

#[derive(Debug, Serialize)]
pub struct StoryInstanceView {
    pub story_id: String,
    pub base_revision: u64,
    pub pack_id: String,
    pub player_role_id: String,
    pub current_scene: String,
    pub opening: StoryOpeningView,
}

#[derive(Debug, Serialize)]
pub struct StoryView {
    pub story_id: String,
    pub base_revision: u64,
    pub premise: String,
    pub current_scene: String,
    pub player_role_id: String,
    pub opening: Option<StoryOpeningView>,
    pub turns: Vec<StoryTurnView>,
    pub next_turn_after: Option<u64>,
    pub roles: Vec<RoleStateView>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetStoryQuery {
    pub turn_after: Option<u64>,
    pub turn_limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct RoleStateView {
    pub role_id: String,
    pub name: String,
    pub source_character_id: Option<String>,
    pub location: String,
    pub goals: Vec<String>,
    pub attributes: Vec<AttributeView>,
}

#[derive(Debug, Serialize)]
pub struct AttributeView {
    pub key: String,
    pub value: String,
}

pub async fn create_story_instance(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateStoryInstanceRequest>,
) -> Result<(StatusCode, Json<StoryInstanceView>), ApiError> {
    let pack_id = PackId::try_new(req.pack_id.clone()).map_err(|error| ApiError::BadRequest(error.to_string()))?;
    let player_id = PlayerId::try_new(req.player_id).map_err(|error| ApiError::BadRequest(error.to_string()))?;
    let player_role_id =
        RoleId::try_new(req.player_role_id.clone()).map_err(|error| ApiError::BadRequest(error.to_string()))?;
    let mut role_profile_selections = BTreeMap::new();
    for selection in req.role_profiles {
        let role_id = RoleId::try_new(selection.role_id).map_err(|error| ApiError::BadRequest(error.to_string()))?;
        let reference = FrozenCharacterCardRef {
            character_id: aise::domain::ids::CharacterId::try_new(selection.character_id)
                .map_err(|error| ApiError::BadRequest(error.to_string()))?,
            version: aise::domain::asset::ids::SemanticVersion::try_new(selection.version)
                .map_err(|error| ApiError::BadRequest(error.to_string()))?,
            digest: aise::domain::asset::ids::Sha256Digest::try_new(&selection.digest)
                .map_err(|error| ApiError::BadRequest(error.to_string()))?,
        };
        if role_profile_selections.insert(role_id.clone(), reference).is_some() {
            return Err(ApiError::BadRequest(format!("duplicate role_profiles.role_id: {role_id}")));
        }
    }
    let instance_factory = state
        .instance_factory
        .as_ref()
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("instance factory not initialized")))?;
    let spec = CreateStoryInstanceSpec {
        pack_id,
        player_id,
        player_role_id: player_role_id.clone(),
        role_profile_selections,
        created_at_ms: now_millis(),
    };
    let info = instance_factory.create(spec).await.map_err(map_instantiation_error)?;
    let store = state.engine.store();
    let limits = SnapshotLimits::from_config(
        &state.engine.config().content,
        &state.engine.config().context,
        &state.engine.config().assets,
        &state.engine.config().narrative,
    );
    let snapshot = store
        .load_story_snapshot(&info.story_id, limits)
        .await
        .map_err(|error| ApiError::Internal(anyhow::anyhow!(error.to_string())))?;
    Ok((
        StatusCode::CREATED,
        Json(StoryInstanceView {
            story_id: info.story_id.to_string(),
            base_revision: info.base_revision.get(),
            pack_id: snapshot.pack().pack_id.to_string(),
            player_role_id: player_role_id.to_string(),
            current_scene: snapshot.current_scene().description.to_string(),
            opening: snapshot
                .story_continuity()
                .recent_segments()
                .first()
                .map(|segment| StoryOpeningView {
                    sequence: segment.sequence,
                    story_text: segment.text.to_string(),
                    created_at: info.created_at_ms,
                })
                .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("story opening was not persisted")))?,
        }),
    ))
}

pub async fn get_story(
    State(state): State<Arc<AppState>>,
    Path(story_id): Path<String>,
    Query(query): Query<GetStoryQuery>,
) -> Result<Json<StoryView>, ApiError> {
    let story_id = StoryId::try_new(story_id).map_err(|error| ApiError::BadRequest(error.to_string()))?;
    let limits = SnapshotLimits::from_config(
        &state.engine.config().content,
        &state.engine.config().context,
        &state.engine.config().assets,
        &state.engine.config().narrative,
    );
    let snapshot = state
        .engine
        .store()
        .load_story_snapshot(&story_id, limits)
        .await
        .map_err(|error| match error {
            aise::persistence::StoreError::NotFound => ApiError::NotFound("story not found".into()),
            other => ApiError::Internal(anyhow::anyhow!(other.to_string())),
        })?;
    let roles = snapshot
        .roles()
        .values()
        .map(|role| {
            let attributes = role
                .state
                .attributes
                .iter()
                .map(|(key, value)| {
                    serde_json::to_string(value)
                        .map(|value| AttributeView {
                            key: key.to_string(),
                            value,
                        })
                        .map_err(|error| ApiError::Internal(error.into()))
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(RoleStateView {
                role_id: role.role_id.to_string(),
                name: role.effective_profile.name.to_string(),
                source_character_id: role.source_character_id.as_ref().map(ToString::to_string),
                location: role.state.location.to_string(),
                goals: role.state.goals.iter().map(ToString::to_string).collect(),
                attributes,
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    let history_config = &state.engine.config().story_history;
    let limit = query
        .turn_limit
        .unwrap_or(history_config.default_page_size)
        .min(history_config.max_page_size);
    if limit == 0 {
        return Err(ApiError::BadRequest("turn_limit must be positive".into()));
    }
    let after_sequence = match query.turn_after {
        None | Some(0) => None,
        Some(value) => Some(StorySequence::try_new(value).map_err(|error| ApiError::BadRequest(error.to_string()))?),
    };
    let history_reader = state
        .story_history_reader
        .as_ref()
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("story history reader not initialized")))?;
    let history = history_reader
        .load_story_history(&story_id, StoryHistoryQuery { after_sequence, limit })
        .await
        .map_err(|error| ApiError::Internal(anyhow::anyhow!(error.to_string())))?;
    Ok(Json(StoryView {
        story_id: snapshot.story_id().to_string(),
        base_revision: snapshot.base_revision().get(),
        premise: snapshot.story_profile().premise.to_string(),
        current_scene: snapshot.current_scene().description.to_string(),
        player_role_id: snapshot.player_role_id().to_string(),
        opening: history.opening,
        turns: history.turns,
        next_turn_after: history.next_after_sequence.map(|sequence| sequence.get()),
        roles,
    }))
}

fn map_instantiation_error(error: StoryInstantiationError) -> ApiError {
    match error {
        StoryInstantiationError::PackNotFound => ApiError::NotFound("pack not found".into()),
        StoryInstantiationError::RoleNotFound { .. } | StoryInstantiationError::RoleNotPlayable { .. } => {
            ApiError::BadRequest("invalid player role".into())
        }
        StoryInstantiationError::CharacterCardNotFound => {
            ApiError::Unprocessable("character card was not found".into())
        }
        StoryInstantiationError::CharacterCardReferenceMismatch => {
            ApiError::Unprocessable("character card reference does not match stored content".into())
        }
        StoryInstantiationError::LimitExceeded { limit } => {
            ApiError::Unprocessable(format!("story instantiation limit exceeded: {limit}"))
        }
        other => ApiError::Unprocessable(other.to_string()),
    }
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}
